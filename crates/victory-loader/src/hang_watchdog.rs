//! The title's hang watchdog (`VICTORY_HANG_WATCHDOG_S=<seconds>`, off
//! without it; issue #344).
//!
//! What the title does (game_scarlett_master.exe 1.0.2.1, image offsets):
//!
//! - A thread of its own, +0x2215a60 (started by +0x2215a20), loops on
//!   `Sleep(1000)` between two `QueryPerformanceCounter`s. A sleep that took
//!   more than 2 s (+0x6159bd4; the process was stopped) or a resume
//!   notification resets it; a suspend notification (+0x22159c0, events
//!   0x24/0x25) holds it.
//! - With no scope open (+0x71b6334 = 0, the scopes +0xb5b6b0/+0xb5b6f0
//!   have their own 300 s limit) it reads a heartbeat, +0x71b6338, which
//!   +0x22159f0 sets to the time; the game's frame (+0x120790) calls it.
//!   After 30 checks in a row (`mov ebp, 1Eh`) with the heartbeat
//!   unchanged, about 30 s without a frame, it calls the handler at
//!   +0x71b6348 and writes to address 0.
//! - The main thread's frame waits for the render thread at +0x23cf6a0
//!   (`WaitForSingleObject(+0xa210cf8, 0)` and `Sleep(1)` in a loop), and
//!   sets that handler to +0x23cf620 for as long: "GPU Loc Sig", then
//!   `mov dword [0], 4D8BD421h` (+0x23cf683, the crash of #344).
//!
//! So one frame of more than 30 s (here: shader compiles, a slow load at
//! 0.3 frames a second) is a GPU lockup to the title. Here the count, set
//! at nine places in both copies of the loop, becomes the seconds asked
//! for; a frame that never ends is still caught, later. Only for build
//! [`BUILD`], and only if every site's bytes are the expected ones.

use crate::frame_rate::Patched;
use crate::resolution::Site;

/// The untouched executable's BLAKE3 the offsets are of (1.0.2.1).
pub const BUILD: &str = "b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93";

/// The title's own count: checks a second apart.
pub const TITLE_SECONDS: u32 = 30;

const fn count(at: usize, bytes: &'static [u8]) -> Site {
    Site { at, what: "hang watchdog's count", bytes }
}

/// Every `mov r32, 1Eh` of +0x2215a60: `ebp` the count, `ebx`/`eax` what
/// it is set back to when the heartbeat moved.
pub const COUNTS: [Site; 9] = [
    count(0x2215acc, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215b3a, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215b62, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215bfd, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215c64, &[0xbb, 0x1e, 0, 0, 0]),
    count(0x2215cde, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215d06, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215d67, &[0xbd, 0x1e, 0, 0, 0]),
    count(0x2215d78, &[0xb8, 0x1e, 0, 0, 0]),
];

/// `VICTORY_HANG_WATCHDOG_S=<seconds>`, 1 to 86400. `None` if unset or out
/// of range (said why).
pub fn asked_seconds() -> Option<u32> {
    let spec = std::env::var("VICTORY_HANG_WATCHDOG_S").ok()?;
    let seconds = parse_seconds(&spec);
    if seconds.is_none() {
        eprintln!("VIctory: VICTORY_HANG_WATCHDOG_S={spec}: expected seconds from 1 to 86400 (the title's own: 30)");
    }
    seconds
}

pub fn parse_seconds(spec: &str) -> Option<u32> {
    spec.trim().parse::<u32>().ok().filter(|s| (1..=86_400).contains(s))
}

/// What goes at a count's site: the same `mov`, `seconds` in place of 30.
pub fn count_code(site: &Site, seconds: u32) -> Vec<u8> {
    let mut c = vec![site.bytes[0]];
    c.extend(seconds.to_le_bytes());
    c
}

/// Applies the switch to the image of `size` bytes at `base`, made from the
/// executable whose BLAKE3 is `hash`; a line in the log and on standard
/// output.
///
/// # Safety
/// The image stays mapped and no thread runs its code yet.
#[cfg(windows)]
pub unsafe fn apply_asked(hash: &str, base: usize, size: usize) {
    use crate::resolution::{matches, write};
    let Some(seconds) = asked_seconds() else {
        return;
    };
    if hash != BUILD {
        println!("hang watchdog: not patched, another build of the title");
        return;
    }
    // All or none: a count set back to 30 at one place and not another
    // would be neither.
    // SAFETY: the caller's promise.
    if let Some(site) = COUNTS.iter().find(|s| unsafe { matches(base, size, s) } != Patched::Done) {
        tracing::warn!(site = %format_args!("{:#x}", site.at), "hang watchdog: not patched, the title's bytes there are not the ones expected");
        println!("hang watchdog: not patched, the title's bytes at +{:#x} are not the ones expected", site.at);
        return;
    }
    // SAFETY: the caller's promise.
    let done = COUNTS.iter().filter(|s| unsafe { write(base, size, s, &count_code(s, seconds)) } == Patched::Done).count();
    tracing::info!(seconds, done, "hang watchdog: the title's 30 s without a frame patched (VICTORY_HANG_WATCHDOG_S)");
    println!("hang watchdog: {seconds} s without a frame in place of the title's {TITLE_SECONDS} ({done} of {} counts patched)", COUNTS.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch() {
        assert_eq!(parse_seconds("600"), Some(600));
        assert_eq!(parse_seconds(" 30 "), Some(30));
        assert_eq!(parse_seconds("0"), None);
        assert_eq!(parse_seconds("86401"), None);
        assert_eq!(parse_seconds("long"), None);
    }

    #[test]
    fn the_counts_are_movs_of_30_and_keep_their_register() {
        for site in &COUNTS {
            assert!(matches!(site.bytes[0], 0xb8 | 0xbb | 0xbd), "mov eax/ebx/ebp, imm32");
            assert_eq!(site.bytes[1..], TITLE_SECONDS.to_le_bytes());
            let c = count_code(site, 600);
            assert_eq!(c.len(), site.bytes.len());
            assert_eq!(c[0], site.bytes[0]);
            assert_eq!(c[1..], 600u32.to_le_bytes());
        }
        // Inside the watchdog thread, +0x2215a60..+0x2215e09.
        assert!(COUNTS.iter().all(|s| (0x2215a60..0x2215e09).contains(&s.at)));
    }
}
