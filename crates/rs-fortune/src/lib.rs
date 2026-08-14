use rand::seq::SliceRandom;

pub fn choose_fortune(text: &str) -> Option<String> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");

    let fortunes: Vec<&str> =
        normalized.split("\n%\n").map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    fortunes.choose(&mut rand::thread_rng()).map(|s| s.to_string())
}
