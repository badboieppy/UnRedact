use std::sync::OnceLock;

#[inline]
pub fn shaping_features() -> &'static [rustybuzz::Feature] {
    static FEATURES: OnceLock<Vec<rustybuzz::Feature>> = OnceLock::new();
    FEATURES
        .get_or_init(|| {
            vec![
                rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"kern"), 1, ..),
                rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"liga"), 1, ..),
                rustybuzz::Feature::new(rustybuzz::Tag::from_bytes(b"clig"), 1, ..),
            ]
        })
        .as_slice()
}
