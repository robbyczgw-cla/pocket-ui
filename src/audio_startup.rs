//! Android heavy-core boot reserve, returned through quiet stereo intervals or a bounded pitch drain.

use std::collections::VecDeque;

pub(super) fn target_frames(system: &str) -> usize {
    // N64 continuous music does not return a large reserve within 20 s, so
    // it stays off. Saturn's measured 65.125 ms burst plus normal PCM and
    // one delivery fits in 132 ms; two device probes return it within 20 s.
    // This smaller Sega candidate still needs a real <=20 s return-to-latency check.
    match system { "saturn" => 48_000*132/1000, _ => 0 }
}

/// A continuous score may contain no removable silence. Drain at one cent,
/// independently of the ordinary queue controller, whose larger skew is disabled.
pub(super) fn drain_step(nominal: f64) -> f64 { nominal * 2.0_f64.powf(1.0 / 1200.0) }

pub(super) fn writer_target(regular: usize, boot: usize, already_played: bool) -> usize {
    if already_played { regular } else { regular.max(boot) }
}

pub(super) struct Trim {
    pub frames: usize,
    pub peak: u16,
    pub nonzero_samples: usize,
    pub splice_step: u16,
}

pub(super) fn trim_silence(queue: &mut VecDeque<i16>, frames: usize) -> Option<Trim> {
    // Stereo peak <=32 is below -60 dBFS. Require a genuine 50 ms interval,
    // retaining 5 ms guards and a 5 ms crossfade at its splice.
    let (guard, fade, minimum) = (240, 240, 2400);
    let mut run = 0;
    let mut peak = 0;
    for at in 0..=queue.len() / 2 {
        let p=if at<queue.len()/2 {queue[at*2].unsigned_abs().max(queue[at*2+1].unsigned_abs())} else {u16::MAX};
        let quiet=p<=32;
        if quiet {
            run+=1;peak=peak.max(p);
            if run<minimum.max(frames+guard*2+fade) {continue;}
        } else if run<minimum {run=0;peak=0;continue;}
        if frames>0 && run>=minimum {
            let take=frames.min(run-guard*2-fade);
            let end=if quiet {at+1} else {at};
            let begin=end-run+guard;
            for j in 0..fade {
                for channel in 0..2 {
                    let left=i32::from(queue[(begin+j)*2+channel]);
                    let right=i32::from(queue[(begin+take+j)*2+channel]);
                    queue[(begin+j)*2+channel]=((left*(fade-1-j) as i32+right*j as i32)/(fade-1) as i32) as i16;
                }
            }
            let mut trim=Trim{frames:take,peak,nonzero_samples:0,splice_step:0};
            for sample in queue.drain((begin+fade)*2..(begin+fade+take)*2) {
                trim.nonzero_samples+=usize::from(sample!=0);
            }
            for at in begin..=begin+fade {
                for channel in 0..2 {
                    trim.splice_step=trim.splice_step.max((i32::from(queue[at*2+channel])-i32::from(queue[(at-1)*2+channel])).unsigned_abs() as u16);
                }
            }
            return Some(trim);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_n64_queue_leaves_the_boot_supply_gap_as_a_known_limit() {
        // Withdrawal of the 352 ms reserve deliberately restores this startup
        // limitation: 249.83 ms of missing supply does not fit normal latency.
        let regular = 48_000 * 48 / 1000;
        let target = writer_target(regular, target_frames("n64"), false);
        let mut queue: VecDeque<_> = vec![1000; (target + 800) * 2].into();
        let missing = (0..11_992 * 2).filter(|_| queue.pop_front().is_none()).count() / 2;
        assert_eq!(missing, 11_992 - regular - 800);
        assert_eq!(target, regular, "a cold N64 start keeps normal game latency");
        assert_eq!(writer_target(regular, target_frames("n64"), true), regular);
    }

    #[test]
    fn sega_rally_boot_gap_does_not_require_n64_latency() {
        // Measured without reserve: 3,126 missing frames (65.125 ms),
        // plus the regular 48 ms and one 60 Hz delivery fit within 132 ms.
        let target=writer_target(2304,target_frames("saturn"),false);
        assert!(target<=6336,"the measured Saturn gap must not borrow the 352 ms N64 reserve");
        let mut queue:VecDeque<_>=vec![1200,-1200].repeat(target).into();
        for _ in 0..3126 {assert_eq!(queue.pop_front(),Some(1200));assert_eq!(queue.pop_front(),Some(-1200));}
        assert!(queue.len()/2>=2304,"the measured gap leaves normal PCM behind");
    }

    #[test]
    fn boot_reserve_returns_through_silence_with_both_tones_bit_identical() {
        let first: Vec<_> = (0..4800).flat_map(|i| [1000 + (i % 40) as i16, -2000]).collect();
        let last: Vec<_> = (0..4800).flat_map(|i| [-3000, 4000 + (i % 40) as i16]).collect();
        let mut queue: VecDeque<_> = first.iter().copied().chain(vec![0; 9600 * 2]).chain(last.iter().copied()).collect();
        let trim = trim_silence(&mut queue, 3072).expect("the quiet interval returns the reserve");
        assert_eq!((trim.frames, trim.peak, trim.nonzero_samples), (3072, 0, 0));
        let heard: Vec<_> = queue.into_iter().collect();
        assert_eq!(&heard[..first.len()], &first);
        assert_eq!(&heard[heard.len() - last.len()..], &last);
        assert_eq!(heard.len(), first.len() + (9600 - 3072) * 2 + last.len());
        assert!(heard[first.len()..heard.len() - last.len()].iter().all(|&v| v == 0));
    }

    #[test]
    fn a_quiet_music_gap_can_return_part_of_the_reserve_without_a_click() {
        let first=vec![1000,-2000].repeat(4800);
        let last=vec![-3000,4000].repeat(4800);
        let quiet:Vec<_>=(0..2880).flat_map(|i| {
            let v=(31.0*(std::f64::consts::TAU*67.0*i as f64/48000.0).sin()).round() as i16;
            [v,-v]
        }).collect();
        let mut queue:VecDeque<_>=first.iter().chain(&quiet).chain(&last).copied().collect();
        let trim=trim_silence(&mut queue,4000).expect("at least 50 ms below -60 dBFS must return a safe part of the reserve");
        assert!(trim.frames>0 && trim.frames<4000,"a short quiet interval must not require the entire reserve");
        assert!(trim.peak<=32 && trim.nonzero_samples>0,"measure the actual discarded quiet music");
        let heard:Vec<_>=queue.into_iter().collect();
        assert_eq!(&heard[..first.len()],&first);
        assert_eq!(&heard[heard.len()-last.len()..],&last);
        let middle=&heard[first.len()..heard.len()-last.len()];
        let mut largest_jump=0;
        for channel in 0..2 {
            let mono:Vec<_>=middle.iter().skip(channel).step_by(2).copied().collect();
            let jump=mono.windows(2).map(|p|(i32::from(p[1])-i32::from(p[0])).abs()).max().unwrap();
            largest_jump=largest_jump.max(jump);
            assert!(jump<=2,"a short crossfade must avoid a new splice click: {jump} PCM units");
        }
        assert_eq!(trim.splice_step as i32,largest_jump,"the live diagnostic measures the actual splice");
        eprintln!("quiet splice: removed={} frames, measured peak={} ({:.3} dBFS), quiet={}ms, crossfade=5ms, largest adjacent step={} PCM units",trim.frames,trim.peak,20.0*(f64::from(trim.peak)/32768.0).log10(),60,largest_jump);
    }

    #[test]
    fn a_short_or_louder_gap_does_not_cut_music() {
        for (frames,level) in [(2399,20),(3000,33)] {
            let mut queue:VecDeque<_>=[vec![1000;400],vec![level;frames*2],vec![-1000;400]].concat().into();
            let before=queue.clone();
            assert!(trim_silence(&mut queue,400).is_none());
            assert_eq!(queue,before);
        }
    }

    #[test]
    fn zero_crossings_or_one_silent_channel_cannot_shorten_a_note() {
        for frame in [[0, 1000], [1000, 0], [0, 0]] {
            let mut queue: VecDeque<_> = (0..2000).flat_map(|_| [1000, -1000].into_iter().chain(frame)).collect();
            let before = queue.clone();
            assert!(trim_silence(&mut queue, 1).is_none());
            assert_eq!(queue, before);
        }
        for system in ["gb", "gba", "snes", "nds", "psp"] { assert_eq!(target_frames(system), 0); }
    }
}
