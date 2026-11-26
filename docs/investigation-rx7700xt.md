# Getting 1.0.2.1 past the loading screen on an RX 7700 XT (running notes)

Kept up to date during the investigation. Newest findings at the end of each section.

## Machine and files

- Windows 11, AMD Radeon RX 7700 XT (RDNA3, 12 GB), Ryzen 5 7600X, 31 GB RAM.
  Two monitors: DISPLAY1 primary 2560x1440, DISPLAY2 1920x1080 (left).
- Game folder in use: `C:\Coding\GTA6 Game files\Grand Theft Auto VI (fixed)` (hard links).
  Package 1.0.2.1; exe must be the untouched one, BLAKE3 `b99f876b74d4a77f…`
  (the emulator then prints `build Series 1.0.2.1`).
- The first USB copy was mangled: every DLL had an `XBE1` header and every `.rpf` an
  `XRP1` header (89 bytes) in front, with the file's original length kept, so the end
  of each file was lost. The user re-copied clean files (`fixed dll`, `fixedrpf`) and
  20 files that had not been copied at all (shaders/packs/textures/script rpf,
  story000_* packs, vcruntime/XboxBvhBuilder/XCurl dlls, …).
  `pack\story000\story000.rpf` is still in the wrapped form (header + complete archive).
- `appdata0.rpf` is genuinely 8,880 bytes (a fresh copy from the Xbox matched byte for
  byte). The game reads past its end at boot (8,192 from 4,096, then 3,408 at 8,880);
  that is read-ahead, not a truncated file. Ruled out.

## Emulator changes made on branch `wrapped-package-files`

- `XBE1`/`XRP1` wrapped files are read through (loader skips the DLL header; file
  layer shifts archive offsets, reports full size, zero-fills the lost end).
- `VICTORY_MONITOR=<n>` puts the window on `\\.\DISPLAY<n>`.
- Host pads read at rest while another program has focus; `VICTORY_NO_PADS=1`
  ignores host pads (keyboard stand-in pad only).
- Diagnostics: async calls not finished after 5 s; failed/empty title reads; first
  60 reads; XUser calls; game-save calls; `VICTORY_STACK_REGS=<+rva>` prints
  callee-saved registers (and what they point to) at that frame in `VICTORY_STACKS`
  dumps.
- Merged Belobug `main` (10 commits).

## How to drive a run without a person

- Diag build: `cargo build --profile diag -p victory --features diagnostics`.
- `VICTORY_KEYS_FILE=<file>`: each line appended later is pressed once (0.2 s).
  Names: `Enter Esc Tab Back Up Down Left Right` or a letter/digit
  (`Escape`/`Backspace` are NOT valid and are silently ignored).
- `VICTORY_CAPTURE=<dir>` + `VICTORY_CAPTURE_FRAMES=60,120,…` to see the screen.
- First-boot sequence: Enter through narration prompt, intro art, brightness,
  "FINISH SETUP?" (A). On LEGAL NOTICES: `Down`, wait ~7 s, `Down`, `Enter`.
  Timing varies run to run (Legal Notices at 40–75 s), so drive it live from captures.
- `VICTORY_FPS=30` matches the console pace (fork default caps at 240).

## What happens

- Hardware D3D12 path: boot → intro → narration prompt → brightness → finish setup →
  Legal Notices (lock on Copyright, Next greyed with countdown) → dark screen with a
  small spinner forever (5 draws/frame, 0 disk reads, 1 core busy).
  The account-picker / start screen never shows its prompts on hardware.
- WARP (`VICTORY_D3D12_WARP=1`, ~25 fps on the CPU): the start screen shows
  "ACCOUNT PICKER (Y) / SCREEN NARRATION (X) / PLAYER (A)"; Legal Notices loads fully;
  after Next it ALSO sits on the dark spinner (same as Marvisak/VIctory#1063 reports).
- Same result on: this fork, Marvisak main (`63bcca4`), Marvisak `ffe310e`,
  codec-d/cool_fox (run by the user). Not a regression.

## Where it is stuck (from stacks + disassembly of 1.0.2.1)

- Main thread: `+0x1621ae0` (.zmt resource loader) → stream read `+0x51d2260` →
  device read `+0x13308e0` (inflate loop) → `+0x1330cd0` (chunk loop, picks a part
  device by index) → device vtable +0x30 → `+0x1331210` → …
- Stream object at the stuck frame: position 0, 0 bytes buffered, 0x1000 buffer: its
  first read never returns. Registers at `+0x51d2319`: r15 =
  `platform:/levels/americas/navmeshes/navmeshes_swaps.zmt` (r14 = length 55).
- Device vtable `+0x62fd480` = the title's codec provider (#1063). No RTTI in the exe.
- `+0x13308e0` loops while compressed bytes remain; if the raw read
  (`+0x1330b60`) returns 0, it never ends. #1063: `FUN_141330cd0`'s guard fails and
  `[inst+0x20]` only drops on success; the producer is title code (reader vtable
  `+0x62fc550`, `+0xcdd930` → `+0x313fc0` sync ReadFile reader / global block reader)
  and leaves the stream buffer empty. During the spin no ReadFile/DStorage call is made.
- Not the cause (measured): emulator version, frame pacing, network (blocked, only a
  hostname lookup), saves (provider only, no containers), XUser (all calls in first
  10 s), XAsync (nothing pending), package chunks (all "installed"), Windows locale
  (en-US), monitor/pads.

## ROOT CAUSE (2026-11-26, ~03:45): navmeshes.rpf is cut short

- Following the loop down: `+0x1330b60` (raw compressed read) → plain path
  `+0x1330c10` → `reader->vt[0x60]` = `+0xcdd930`, which returns 0 when the file
  object is -1 or **when the offset is at or past the file object's size**.
  The inflate loop in `+0x13308e0` then never ends (input stays empty, no error).
- `VICTORY_STACK_REGS=+0x133096e` with `VICTORY_STACK_PEEK=rbx+8>0,rbx+8>0x18,…`
  (rbx = the codec provider, [rbx+8] = the archive's storage object) shows:
  storage `+0x28` = archive size `0x45ce0000`; part table `+0x50` = null, so the
  default file `+0x18` is used; that file object's size `+8` = `0x45ce0000`.
  That is exactly the size of our `navmeshes.rpf` (1,171,128,320 bytes).
- So the table of contents of navmeshes.rpf places `navmeshes_swaps.zmt` (204,848
  compressed bytes) past the end of the file we have: **the copy is truncated**.
  `0x45ce0000` is an exact multiple of 64 KiB; no complete archive here is.
- Only one other file has a 64 KiB-multiple size: `data\ttdb\peds\decorations_1_3.ttpf`
  (`0x1e660000`) — suspicious, recopy it too.
- Fix: copy `navmeshes.rpf` (and `decorations_1_3.ttpf`) from the Xbox again and check
  the new size is larger and not a multiple of 64 KiB. No emulator change can supply
  the missing data.
- Also done: `pack\story000\story000.rpf` in the (fixed) folder is now the plain
  archive (wrapped header stripped); the wrapped original stays in the original folder.

## Leads

- Marvisak/VIctory#1063: same loader; on GTX 1660 it stops at `+0x1330a44` (codec
  0x817, `platform:/levels/americas/map/powergrids.zmt`); WARP passes Legal Notices;
  ZSayerHQ/victory-world-runtime is said to reach the world on RDNA3/RTX30+/5090.
- Fork survey in progress (fixes for the loader hang, AMD RDNA3 world runs, speed).
