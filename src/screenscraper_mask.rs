// Shared by build.rs (`include!`) and src/screenscraper.rs: how ScreenScraper's developer id is kept in the binary.
// XORed with these bytes so it does not stand in the program as plain text (`strings`). This hides it from a glance, not
// from someone who wants it; every frontend that ships a ScreenScraper client carries its id this way or plainer.

const SCREENSCRAPER_MASK: &[u8] = b"pocket-ui/screenscraper: not a secret, only not plain text";

/// `data` XORed with [`SCREENSCRAPER_MASK`]: the same call packs and unpacks.
pub fn screenscraper_mask(data: &[u8]) -> Vec<u8> {
    data.iter().enumerate().map(|(i, b)| b ^ SCREENSCRAPER_MASK[i % SCREENSCRAPER_MASK.len()]).collect()
}
