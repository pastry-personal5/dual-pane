/// One of the two independently browsable workspace Browsers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrowserSide {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::BrowserSide;

    #[test]
    fn sides_are_copyable_and_distinct() {
        let left = BrowserSide::Left;
        assert_eq!(left, BrowserSide::Left);
        assert_ne!(left, BrowserSide::Right);
    }
}
