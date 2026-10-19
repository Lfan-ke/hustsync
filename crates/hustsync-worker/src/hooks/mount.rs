//! Which filesystem is mounted exactly at a directory, from `/proc/self/mountinfo`.

use std::{io, os::unix::ffi::OsStrExt, path::Path};

/// The filesystem type mounted at `dir`, or `None` when `dir` is not a mount point.
pub fn fs_type(dir: &Path) -> io::Result<Option<String>> {
    let dir = dir.canonicalize()?;
    let info = std::fs::read("/proc/self/mountinfo")?;
    Ok(find(&info, dir.as_os_str().as_bytes()))
}

/// The last mount at `point` wins, as later mounts cover earlier ones.
fn find(info: &[u8], point: &[u8]) -> Option<String> {
    info.split(|&b| b == b'\n').rev().find_map(|line| {
        let sep = line.windows(3).position(|w| w == b" - ")?;
        let at = line[..sep].split(|&b| b == b' ').nth(4)?;
        let fs = line[sep + 3..].split(|&b| b == b' ').next()?;
        (unescape(at) == point).then(|| String::from_utf8_lossy(fs).into_owned())
    })
}

/// mountinfo writes space, tab, newline and backslash as three octal digits after a backslash.
fn unescape(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let digits = s
            .get(i + 1..i + 4)
            .filter(|d| d.iter().all(|c| (b'0'..=b'7').contains(c)));
        match (s[i], digits) {
            (b'\\', Some(d)) => {
                out.push(
                    d.iter()
                        .fold(0u8, |n, c| n.wrapping_mul(8).wrapping_add(c - b'0')),
                );
                i += 4;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const INFO: &[u8] = b"\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
40 22 0:35 / /srv/mirror\\040one rw,relatime shared:20 - zfs tank/one rw,xattr
41 22 0:36 / /srv/two rw,relatime shared:21 - ext4 /dev/sdb1 rw
42 41 0:37 / /srv/two rw,relatime shared:22 - zfs tank/two rw
";

    #[test]
    fn finds_the_type_mounted_at_a_point() {
        assert_eq!(find(INFO, b"/srv/mirror one").as_deref(), Some("zfs"));
        assert_eq!(find(INFO, b"/").as_deref(), Some("ext4"));
    }

    #[test]
    fn the_last_mount_on_a_point_wins() {
        assert_eq!(find(INFO, b"/srv/two").as_deref(), Some("zfs"));
    }

    #[test]
    fn a_plain_directory_is_no_mount_point() {
        assert_eq!(find(INFO, b"/srv"), None);
        assert_eq!(find(INFO, b"/srv/mirror\\040one"), None);
    }

    #[test]
    fn unescapes_octal() {
        assert_eq!(unescape(br"a\040b\011c\134d"), b"a b\tc\\d");
        assert_eq!(unescape(br"keep\09"), br"keep\09");
    }

    #[test]
    fn reads_this_system() {
        assert!(fs_type(Path::new("/")).unwrap().is_some());
        assert!(fs_type(Path::new("/proc")).unwrap().as_deref() == Some("proc"));
    }
}
