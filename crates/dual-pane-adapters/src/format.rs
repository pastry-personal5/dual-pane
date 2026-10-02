//! Display wording for Folder Items fields, summaries, and locations.

use dual_pane_domain::{Entry, Location};
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// Shown where a value is unavailable.
pub const UNAVAILABLE: &str = "—";

/// A path for display. Bytes that are not valid UTF-8 appear as U+FFFD.
pub fn location_text(location: &Location) -> String {
    if location.is_root() {
        return "/".to_owned();
    }
    location.components().iter().fold(String::new(), |mut text, name| {
        text.push('/');
        text.push_str(&name.to_text_lossy());
        text
    })
}

/// A size in decimal units: whole bytes below 1 KB, then one decimal place,
/// such as `512 B` or `112.5 KB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    // Round before choosing the unit, so 999,999 bytes reads `1.0 MB`.
    while value >= 999.95 && unit + 1 < UNITS.len() {
        value /= 1000.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Compact elapsed time from `modified` to `now`, both in Unix seconds:
/// `now` under a minute or in the future, then `N min`, `N h`, `N d`, `N M`
/// (30-day months below a year), and `N y` (365-day years).
pub fn format_relative(modified: i64, now: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    let elapsed = now.saturating_sub(modified);
    if elapsed < MINUTE {
        return "now".to_owned();
    }
    if elapsed < HOUR {
        return format!("{} min", elapsed / MINUTE);
    }
    if elapsed < DAY {
        return format!("{} h", elapsed / HOUR);
    }
    let days = elapsed / DAY;
    if days < 30 {
        return format!("{days} d");
    }
    if days / 30 < 12 {
        return format!("{} M", days / 30);
    }
    format!("{} y", (days / 365).max(1))
}

/// Position on the Relative Date color strip for an Item's elapsed age.
/// Anchors are one hour, one day, seven days, 30 days, and 365 days.
pub fn relative_age_position(modified: i64, now: i64) -> f64 {
    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;
    const ANCHORS: [i64; 5] = [HOUR, DAY, 7 * DAY, 30 * DAY, 365 * DAY];
    let elapsed = now.saturating_sub(modified);
    if elapsed <= ANCHORS[0] {
        return 0.0;
    }
    for (segment, ages) in ANCHORS.windows(2).enumerate() {
        if elapsed <= ages[1] {
            return (segment as f64 + (elapsed - ages[0]) as f64 / (ages[1] - ages[0]) as f64) / 4.0;
        }
    }
    1.0
}

/// A local timestamp such as `2026-09-30 20:21`, or `—` when it cannot be
/// represented.
pub fn format_exact(seconds: i64, time_zone: &TimeZone) -> String {
    Timestamp::from_second(seconds).map_or_else(|_| UNAVAILABLE.to_owned(), |timestamp| timestamp.to_zoned(time_zone.clone()).strftime("%Y-%m-%d %H:%M").to_string())
}

/// The non-recursive total size of the non-folder Items among `entries`, or
/// `—` when any of their sizes is unknown. Folders, including links to
/// folders, show no size and are left out.
pub fn total_size<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> String {
    entries.into_iter().filter(|entry| !entry.can_enter()).try_fold(0_u64, |total, entry| entry.metadata().size_bytes().map(|size| total.saturating_add(size))).map_or_else(|| UNAVAILABLE.to_owned(), format_size)
}

/// `1 item` or `N items`.
pub fn item_count(count: usize) -> String {
    if count == 1 { "1 item".to_owned() } else { format!("{count} items") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dual_pane_domain::{EntryKind, EntryMetadata, EntryName};

    #[test]
    fn sizes_use_whole_bytes_then_one_decimal_place() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1000), "1.0 KB");
        assert_eq!(format_size(112_500), "112.5 KB");
        assert_eq!(format_size(999_999), "1.0 MB");
        assert_eq!(format_size(4_200_000_000), "4.2 GB");
        assert_eq!(format_size(u64::MAX), "18.4 EB");
    }

    #[test]
    fn relative_dates_use_compact_units() {
        let now = 1_000_000_000;
        let cases = [(now, "now"), (now + 3_600, "now"), (now - 59, "now"), (now - 60, "1 min"), (now - 3_599, "59 min"), (now - 17 * 3_600, "17 h"), (now - 2 * 86_400, "2 d"), (now - 29 * 86_400, "29 d"), (now - 30 * 86_400, "1 M"), (now - 335 * 86_400, "11 M"), (now - 362 * 86_400, "1 y"), (now - 2 * 365 * 86_400, "2 y")];
        for (modified, expected) in cases {
            assert_eq!(format_relative(modified, now), expected, "{modified}");
        }
        assert_eq!(format_relative(i64::MIN, i64::MAX), format!("{} y", i64::MAX / 86_400 / 365));
    }

    #[test]
    fn relative_age_position_interpolates_and_clamps() {
        const DAY: i64 = 86_400;
        let now = 1_000_000_000;
        let anchors = [(3_600, 0.0), (DAY, 0.25), (7 * DAY, 0.5), (30 * DAY, 0.75), (365 * DAY, 1.0)];
        for (age, position) in anchors {
            assert_eq!(relative_age_position(now - age, now), position);
        }
        for segment in anchors.windows(2) {
            let midpoint = (segment[0].0 + segment[1].0) / 2;
            let expected = (segment[0].1 + segment[1].1) / 2.0;
            assert!((relative_age_position(now - midpoint, now) - expected).abs() < 0.00001);
        }
        assert_eq!(relative_age_position(now + DAY, now), 0.0);
        assert_eq!(relative_age_position(now - 3_599, now), 0.0);
        assert_eq!(relative_age_position(now - 366 * DAY, now), 1.0);
        assert_eq!(relative_age_position(i64::MIN, i64::MAX), 1.0);
    }

    #[test]
    fn exact_dates_use_the_given_time_zone() {
        assert_eq!(format_exact(1_790_000_000, &TimeZone::UTC), "2026-09-21 14:13");
        let tokyo = TimeZone::fixed(jiff::tz::offset(9));
        assert_eq!(format_exact(1_790_000_000, &tokyo), "2026-09-21 23:13");
        assert_eq!(format_exact(i64::MAX, &TimeZone::UTC), UNAVAILABLE);
    }

    #[test]
    fn totals_skip_folders_and_are_unknown_when_any_size_is() {
        let file = |name: &str, size| Entry::with_metadata(EntryName::new(name).unwrap(), EntryKind::File, EntryMetadata::new(None, size));
        let folder = Entry::with_metadata(EntryName::new("folder").unwrap(), EntryKind::Directory, EntryMetadata::new(None, None));
        let link = Entry::with_metadata(EntryName::new("link").unwrap(), EntryKind::Symlink { points_to_directory: true }, EntryMetadata::new(None, Some(9)));
        assert_eq!(total_size(&[file("a", Some(600)), file("b", Some(900)), folder.clone(), link.clone()]), "1.5 KB");
        assert_eq!(total_size(&[link]), "0 B", "a link to a folder counts as a folder");
        assert_eq!(total_size(&[file("a", Some(600)), file("b", None)]), UNAVAILABLE);
        assert_eq!(total_size(&[folder]), "0 B");
        assert_eq!(item_count(1), "1 item");
        assert_eq!(item_count(0), "0 items");
    }
}
