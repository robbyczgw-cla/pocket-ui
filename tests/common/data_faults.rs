//! One deterministic file-fault policy, also used by private-reader unit probes.
pub fn faults(seed: &[u8]) -> Vec<(String, Option<Vec<u8>>)> {
    let mut variants = vec![("valid".into(), Some(seed.to_vec())), ("missing".into(), None), ("empty".into(), Some(vec![]))];
    for cut in [1.min(seed.len()), seed.len() / 4, seed.len() / 2, seed.len().saturating_sub(1)] {
        variants.push((format!("cut-{cut}"), Some(seed[..cut].to_vec())));
    }
    for at in [0, seed.len() / 2, seed.len().saturating_sub(1)] {
        let mut bytes = seed.to_vec();
        if let Some(b) = bytes.get_mut(at) { *b ^= 0xff; }
        variants.push((format!("flip-{at}"), Some(bytes)));
    }
    variants.push(("encoding".into(), Some(vec![0xff, 0xfe, 0, 0xd8, 0x80])));
    variants
}
