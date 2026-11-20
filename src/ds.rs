//! DS core choice and separate moments. Battery saves stay shared; moments do not.
use std::path::{Path, PathBuf};

pub const LEGACY_MARK_ENV: &str = "POCKET_DS_LEGACY_MARK";
pub const CORES: &[&str] = &["dsperate", "melondsds"];
pub fn label(core: &str) -> &'static str { if core == "dsperate" { "DSperate" } else { "melonDS" } }
pub const LEGACY_NOTE: &str = "Your earlier saved moments use melonDS. Choose melonDS in Runs on to load them. Your in-game save is shared.";

pub fn default_for(weak_device: bool, android: bool) -> &'static str {
    if weak_device && !android { "dsperate" } else { "melondsds" }
}

pub fn default_core() -> &'static str {
    #[cfg(target_os = "android")]
    { default_for(false, true) }
    #[cfg(not(target_os = "android"))]
    { default_for(cfg!(feature = "brick") || crate::device::current().model.is_some_and(|m| m.small_chip()), false) }
}

/// Beside the release, so updating or rolling back also changes the core.
pub fn bundled_path() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default().join("cores/dsperate_libretro.so")
}

/// Old melonDS files stay byte-for-byte where they were. The DSperate directory
/// cannot collide with a ROM's stem, and works after Android's state-key resolver.
pub fn state_base(legacy: &Path, core: &str) -> PathBuf {
    if core != "dsperate" { return legacy.to_path_buf(); }
    legacy.parent().unwrap_or(Path::new("")).join(".cores/dsperate")
        .join(legacy.file_name().unwrap_or_default())
}

/// Recognize the bounded libretro state payload before another core sees it.
pub fn state_is_dsperate(data: &[u8]) -> bool {
    if data.get(4..8)!=Some(b"DSST") {return false;}
    let Some(size)=data.get(..4).and_then(|b| <[u8;4]>::try_from(b).ok()).map(u32::from_le_bytes) else {return false;};
    size>=8 && (size as usize).checked_add(4).is_some_and(|end| end<=data.len())
}

pub fn check_state(core_name: &str, data: &[u8]) -> Result<(), String> {
    match (core_name,state_is_dsperate(data)) {
        ("DSperate",false)=>Err("This is not a DSperate moment. Existing melonDS moments need melonDS in Runs on.".into()),
        (name,true) if name.starts_with("melonDS")=>Err("This moment needs DSperate in Runs on.".into()),
        _=>Ok(()),
    }
}

pub fn legacy_from_current(current: &Path, core: &str) -> Option<PathBuf> {
    if core!="dsperate" {return None;}
    let namespace=current.parent()?;
    if namespace.file_name()? != "dsperate" || namespace.parent()?.file_name()? != ".cores" {return None;}
    Some(namespace.parent()?.parent()?.join(current.file_name()?))
}

/// Preserve the older identity supplied by the launcher, never infer it from
/// a ROM which may already have been replaced. Existing marks are not changed.
pub fn retain_legacy_mark(current:&Path,core:&str,mark:&crate::frozen::Mark) {
    if mark.core!="melondsds"||mark.emulator!="pocket-core" {return;}
    let legacy=if core=="dsperate" {legacy_from_current(current,core)} else if core=="melondsds" {Some(current.to_path_buf())} else {None};
    let Some(legacy)=legacy else {return;};
    if crate::frozen_phone::read_mark(&legacy).is_none() && legacy_note(&legacy,"dsperate").is_some() {
        crate::frozen_phone::write_mark(&legacy,mark);
    }
}

pub fn note_for_current(current: &Path, core: &str) -> Option<&'static str> {
    legacy_note(&legacy_from_current(current,core)?,core)
}

pub fn legacy_note(legacy: &Path, core: &str) -> Option<&'static str> {
    if core != "dsperate" { return None; }
    let old = crate::slots::auto_candidates(&crate::slots::auto_path(legacy), false).iter().any(|p| p.is_file())
        || (0..crate::slots::SLOTS).any(|s| crate::slots::state_path(legacy, s).is_file())
        || !crate::timeline::entries(legacy).is_empty();
    old.then_some(LEGACY_NOTE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weak_linux_defaults_change_but_android_and_strong_devices_keep_melonds() {
        assert_eq!(default_for(true, false), "dsperate");
        for args in [(false,false),(false,true),(true,true)] {
            assert_eq!(default_for(args.0,args.1), "melondsds");
        }
    }
    #[test]
    fn foreign_or_truncated_moments_are_refused_before_the_core_is_called() {
        let mut state=8u32.to_le_bytes().to_vec();state.extend_from_slice(b"DSST\x01\0\0\0");
        assert!(state_is_dsperate(&state));
        assert!(check_state("DSperate",&state).is_ok());
        assert!(check_state("melonDS DS",&state).unwrap_err().contains("DSperate"));
        assert!(check_state("DSperate",b"legacy melonDS state").unwrap_err().contains("melonDS"));
        assert!(!state_is_dsperate(&state[..state.len()-1]));
        state[..4].copy_from_slice(&u32::MAX.to_le_bytes());assert!(!state_is_dsperate(&state));
        assert!(check_state("mGBA",b"unrelated").is_ok());
    }

    #[test]
    fn switching_cores_keeps_all_existing_moments_and_the_shared_save() {
        let d=crate::testutil::TestDir::new("ds-core-moments");
        let old=d.join("nds/Game");std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        let auto=crate::slots::auto_path(&old);let slot=crate::slots::state_path(&old,0);
        std::fs::write(&auto,b"old melonDS auto").unwrap();
        std::fs::write(&slot,b"old melonDS slot").unwrap();
        let save=d.join("Game.sav");std::fs::write(&save,b"shared battery").unwrap();
        let new=state_base(&old,"dsperate");std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::write(crate::slots::auto_path(&new),b"DSperate auto").unwrap();
        std::fs::write(crate::slots::state_path(&new,0),b"DSperate slot").unwrap();
        assert_eq!(std::fs::read(&auto).unwrap(),b"old melonDS auto");
        assert_eq!(std::fs::read(&slot).unwrap(),b"old melonDS slot");
        assert_eq!(std::fs::read(&save).unwrap(),b"shared battery");
        assert_eq!(state_base(&old,"melondsds"),old);
        assert_eq!(legacy_note(&old,"dsperate"),Some(LEGACY_NOTE));
        assert_eq!(legacy_from_current(&new,"dsperate"),Some(old.clone()));
        assert_eq!(note_for_current(&new,"dsperate"),Some(LEGACY_NOTE));
        assert_eq!(legacy_note(&old,"melondsds"),None);
    }

    #[test]
    fn continue_and_earlier_survive_switching_back_without_trusting_a_replaced_rom() {
        const CHILD: &str = "POCKET_DS_SAVE_TEST";
        let Ok(root) = std::env::var(CHILD) else {
            let d=crate::testutil::TestDir::new("ds-continue");
            let status=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","ds::tests::continue_and_earlier_survive_switching_back_without_trusting_a_replaced_rom","--nocapture"])
                .env(CHILD,d.as_os_str()).env("POCKETUI_STATES",d.join("states")).status().unwrap();
            assert!(status.success());return;
        };
        let rom=Path::new(&root).join("Game.nds");std::fs::write(&rom,b"owned test game identity").unwrap();
        let old=crate::slots::base_for_core("nds","Game","melondsds");
        let new=crate::slots::base_for_core("nds","Game","dsperate");
        crate::state_file::write(&crate::slots::auto_path(&old),b"legacy moment").unwrap();
        let old_mark=crate::frozen::Mark::new("pocket-core","melondsds",&rom,None).unwrap();
        retain_legacy_mark(&new,"dsperate",&old_mark);
        crate::state_file::write(&crate::slots::auto_path(&new),b"new moment").unwrap();
        let new_mark=crate::frozen::Mark::new("pocket-core","dsperate",&rom,None).unwrap();
        crate::frozen_phone::write_mark(&new,&new_mark);
        let earlier=crate::timeline::Name::parse("earlier1").unwrap();
        crate::state_file::write(&crate::timeline::path(&old,earlier),b"old earlier").unwrap();
        crate::state_file::write(&crate::timeline::path(&new,earlier),b"new earlier").unwrap();
        for (core,last_mark,base,expected) in [
            ("dsperate",old_mark,&new,&b"new earlier"[..]),
            ("melondsds",new_mark,&old,&b"old earlier"[..]),
        ] {
            let game=crate::frozen::Game {system:"nds".into(),path:rom.clone(),emulator:"pocket-core".into(),core:core.into(),mark:Some(last_mark)};
            let files=crate::frozen::resume_files(&game);
            assert!(crate::frozen::is_frozen(&game,&files),"Continue with {core}");
            assert_eq!(crate::timeline::entries(base).len(),1);
            assert_eq!(crate::state_file::read(&crate::timeline::path(base,earlier)).unwrap(),expected);
            std::fs::write(&rom,b"a different replacement ROM").unwrap();
            assert!(!crate::frozen::is_frozen(&game,&files),"{core}: don't resume another ROM's moment");
            std::fs::write(&rom,b"owned test game identity").unwrap();
        }
        assert_eq!(crate::state_file::read(&crate::slots::auto_path(&old)).unwrap(),b"legacy moment");
    }
}
