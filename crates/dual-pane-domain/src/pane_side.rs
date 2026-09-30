/// One of the two independently browsable workspace panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneSide {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::PaneSide;

    #[test]
    fn sides_are_copyable_and_distinct() {
        let left = PaneSide::Left;
        assert_eq!(left, PaneSide::Left);
        assert_ne!(left, PaneSide::Right);
    }
}
