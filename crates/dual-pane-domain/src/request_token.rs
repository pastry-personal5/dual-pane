/// Identifies one asynchronous request so that a result can be matched to the
/// request that is still current, and a stale result can be ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestToken(u64);

impl RequestToken {
    /// The first token an issuer hands out.
    pub fn first() -> Self {
        Self(0)
    }

    /// The token issued after `self`. Distinct from every earlier token for
    /// any realistic number of requests.
    #[must_use]
    pub fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}
