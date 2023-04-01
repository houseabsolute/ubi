## 0.0.22

- The `--self-upgrade` option now works on Windows. However, it leaves behind
  an old binary named `ubi-old.exe` that must be deleted manually.

## 0.0.21 - 2023-03-12

- Improved matching of OS and CPU architecture names in release asset
  names. This release should do a better job with more projects.

## 0.0.20 - 2023-03-04

- This release includes a number of changes to support building on many more
  platforms.
  - The full list of architectures that binaries are released for is:
    - FreeBSD x86-64 **new**
    - Linux x86-64
    - Linux aarch64 (aka arm64)
    - Linux arm (32-bit)
    - Linux i586 (x86 32-bit) **new**
    - Linux mips (32-bit) **new**
    - Linux mipsel (32-bit little-endian) **new**
    - Linux mips64 **new**
    - Linux mips64el (little-endian) **new**
    - Linux PowerPC (32-bit) **new**
    - Linux PowerPC64 **new**
    - Linux PowerPC64le (little-endian) **new**
    - Linux riscv64 **new**
    - Linux s390x **new**
    - NetBSD x86-64 **new**
    - Windows x86-64
    - Windows i686 (32-bit) **new**
    - Windows aarch64 (aka arm64) **new**
    - macOS x86-64
    - macOS aarch64 (aka arm64)
  - The code supports some other OS and CPU architectures internally, but I do
    not have any way to build these:
    - Fuchsia x86-64 and aarch64 - not supported by `cross`.
    - Illumos x86-64 - OpenSSL build fails with odd error about `granlib`
      executable.
    - Linux Sparc64 - not supported by OpenSSL.
    - Solaris x86-64 - supported by `cross` but building the
      [`mio`](https://lib.rs/crates/mio) crate fails.
    - Solaris Sparc - not supported by OpenSSL.
  - In order to do this, `ubi` now uses the
    [`openssl`](https://lib.rs/crates/openssl) crate under the hood instead of
    [`rustls`](https://lib.rs/crates/rustls). That's because `rustls` depends
    on [`ring`](https://lib.rs/crates/ring), which does not support nearly as
    many CPU architectures as OpenSSL. The `vendored` feature for the
    `openssl` crate is enabled, which causes it to compile and statically link
    a copy of OpenSSL into the resulting binary. This makes the resulting
    binary more portable at the cost of not using the system OpenSSL.

## 0.0.19 - 2023-02-18

- Fixed handling of bare executables on Windows. It would reject these because
  it wasn't expecting to download a file with a `.exe` extension.

## 0.0.18 - 2023-01-22

- Most errors no longer print out usage information. Now this is only printed
  for errors related to invalid CLI arguments. GH #22.
- Really fix handling of bare xz-compressed binaries. Based on PR #27 from
  Marco Fontani.
- Add support for bare bz-compressed binaries.

## 0.0.17 - 2022-10-29

- Fixed handling of xz-compressed tarballs. These were ignored even though
  there was code to handle them properly. Reported by Danny Kirkham. GH #24.

## 0.0.16 - 2022-10-04

- Fixed matching the "aarch64" architecture for macOS. At least with Go, these
  binaries end up labeled as "arm64" instead of "aarch64", and `ubi` should
  treat that as a match. Reported by Ajay Vijayakumar.

## 0.0.15 - 2022-09-05

- Added a `--self-upgrade` flag, which will use `ubi` to upgrade `ubi`. Note
  that this flag does not work on Windows.

## 0.0.14 - 2022-09-04

- Added a `--url` flag as an alternative to `--project`. This bypasses the
  need for using the GitHub API, so you don't have to worry about the API
  limits. This is a good choice for use in CI.

## 0.0.13 - 2022-09-01

- Releases are now downloaded using the GitHub REST API instead of trying to
  just download a tarball directly. This lets `ubi` download releases from
  private projects.

## 0.0.12 - 2022-07-04

- Bare xz-compressed binaries are now handled properly. Previously ubi would
  download and "install" the compressed file as an executable. Now ubi will
  uncompress this file properly. Based on PR #19 from Marco Fontani.
- Fixed a bug in handling of xz-compressed tarballs. There was some support
  for this, but it wasn't complete. These should now be handled just like
  other compressed tarballs.

## 0.0.11 - 2022-07-03

- Improved handling of urls passed to `--project` so any path that contains an
  org/user and repo works. For example
  `https://github.com/houseabsolute/precious/releases` and
  `https://github.com/BurntSushi/ripgrep/pull/2049` will now work.
- All Linux binaries are now compiled with musl statically linked instead of
  dynamically linking glibc. This should increase portability.
- The Linux ARM target is now just "arm" instead of "armv7", without hard
  floats ("hf"). This should make the ARM binary more portable.

## 0.0.10 - 2022-07-02

- Fixed releases for Linux ARM64 and ARM 32-bit. The 32-bit release was
  getting uploaded as aarch64.

## 0.0.9 - 2022-07-02

- This release includes native binaries for Linux ARM64 and ARM (32-bit).
- Added a new flag, `--matching`, which can be used to pick a specific release
  file when there are multiple matching options for your OS and CPU
  architecture. Based on PR #18 from Marco Fontani. Fixes #17.
- When there multiple matches and `--matching` is not given, the same release
  file will always be picked. Previously this was not guaranteed.
- Improved filtering of 32-bit executables when running on 64-bit machines.

## 0.0.8 - 2022-04-25

- No code changes from the last release. The binary releases built by GitHub
  Actions now build on Ubuntu 18.04 instead of 20.04. This restores
  compatibility with systems using glibc 2.27. Reported by Olaf Alders. GH
  #16.
- This release also includes native ARM64 binaries for macOS 11+.

## 0.0.7 - 2022-04-23

- Include "x64" as a match for the `x86_64` architecture.

## 0.0.6 - 2021-01-15

- Changed CPU architecture matching to be stricter based on the current
  platform's CPU.
- Changed file extension mapping to work of an allowed list of
  extensions. This is stricter than the previous check, which just filtered
  out a few things like `.deb` and `.rpm`.

## 0.0.5 - 2021-01-15

- Include s390 and s390x in possible arch list. This also fixes a bug where
  that arch might be used when running `ubi` on any platform.
- Ignore `.deb` and `.rpm` files.
- Look for multiple valid files to download and prefer 64-bit binaries on
  64-bit CPUs.

## 0.0.4 - 2021-01-15

- Add support for releases which are either the bare executable or a gzipped
  executable, like rust-analyzer.

## 0.0.3 - 2021-01-15

- Update tokio and other async deps to avoid panics and eliminate deprecated
  net2 crate from dep tree.

## 0.0.2 - 2021-01-09

- When running on Windows, add ".exe" to the user-supplied --exe name if it
  doesn't already have it. This makes it simpler to use ubi with the exact
  same invocation across platforms.

## 0.0.1 - 2021-01-07

- First release
