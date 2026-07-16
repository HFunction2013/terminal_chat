#[allow(dead_code)]
pub struct Hook<F> {
    pub name: &'static str,
    pub f: F,
}