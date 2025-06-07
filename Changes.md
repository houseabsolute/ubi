## 0.6.2

- Changed dependency declarations to mostly eliminate default features, which removed quite a few
  transitive dependencies.

## 0.6.1 2025-03-18

- Upgraded the dependency on `zip` to 2.4.1. Previously, `ubi` pinned `zip` 2.2.3, which was yanked,
  because the 2.3.0 release broke `ubi`'s tests.

## 0.6.0 2025-03-16

- The `UbiBuilder::github_token` and `UbiBuilder::gitlab_token` methods are deprecated in favor of a
  new `UbiBuilder::token` method. These deprecated methods will be removed in a future release.
- Added support for release artifacts with a `.jar` extension. Based on PR #110 from @vemoo.
- `ubi` could incorrectly pick a release for Android when running on Linux aarch64 platforms.
  Reported by @jahands (Jacob Hands). GH #111.

## 0.5.2 - 2025-02-22

- This is identical to 0.5.1, but it was necessary to work around a bug in my GitHub release
  automation. The 0.5.1 is no longer available on GitHub, but the crates.io releases still exist and
  there's no issue with using it.

## 0.5.1 - 2025-02-22

- Fixed a bug introduced in the last release where `ubi` could pick a file with a partial match over
  an exact match, but only with zip files. Reported by @oalders (Olaf Alders). GH #100.

## 0.5.0 - 2025-02-15

- On Windows, `ubi` now looks for files with `.bat` extensions as an executable. This applies both
  to releases of standalone files and executables in an archive file (tarball or zip). Based on work
  by @timothysparg (Tim Sparg) in PR #97.
- If there are no files that are an exact match for the project name in an archive file, `ubi` will
  now look for files that _start_ with the project name. On non-Windows, boxes, this will only match
  executables. On Windows, this will only match `.bat` or `.exe` files. Based on work by
  @timothysparg (Tim Sparg) in PR #97.

## 0.4.2 - 2025-02-04

- Added `all` to the list of architecture strings to match against for macOS on ARM64. It looks like
  this is in use in the wild. See https://github.com/segmentio/golines/releases/tag/v0.12.2 for an
  example. Reported by Shyam Subramaniyam.

## 0.4.1 - 2025-01-18

- Fixed a bug where `ubi` where zip files containing a directory that matched the expected
  executable name caused `ubi` to extract the directory instead of the actual executable, resulting
  in a 0-length file. Based on a PR #89 from @fiadliel (Gary Coady). Fixes #88.
- Added a new `UbiBuilder::rename_exe_to` method, along with a `--rename-exe-to` CLI flag. When this
  is set, the installed executable will use the name given here, instead of the name that it has in
  the downloaded file. This is useful for projects that do releases where the executable name
  includes things like a version number of platform information. Based on discussion in #86.
- Added support for release artifacts with a `.pyz` extension. These are zip files containing Python
  code, and they can be directly executed. Based on PR #85 from @itochan (Kazunori Jo).
- Added support for release artifacts with a `.AppImage` extension. These will only be picked when
  running Linux. Requested by @saulh (Saul Reynolds-Haertle). GH #86.
- Fixed a bug where `ubi` would consider an asset with `.exe` extension on non-Windows platforms. In
  practice, this would probably only have been an issue for projects with exactly one release
  artifact, where that artifact had a `.exe` extension.
- The `--extract-all` CLI option added in the previous release did not have any description in the
  help output. This has been fixed.

## 0.4.0 - 2025-01-03

- The `ubi` CLI tool now takes an optional `--extract-all` argument. If this is passed, it will only
  look for archive files and it will extract the entire contents of an archive it finds. There is
  also a new corresponding `UbiBuilder::extract_all` method. Requested by @Entze (Lukas Grassauer).
  GH #68.
- The `UbiBuilder::install_dir` method now takes `AsRef<Path>` instead of `PathBuf`, which should
  make it more convenient to use.
- Previously, `ubi` would create the install directory very early in its process, well before it had
  something to install. This meant that if it failed to find an asset, couldn't download the asset,
  or other errors happened, it would leave this directory behind. Now it creates this directory
  immediately before writing the executable it found to disk.

## 0.3.0 - 2024-12-26

- `ubi` now works with GitLab in addition to GitHub. For the command-line program, if the value you
  pass to `--project` has a `gitlab.com` domain, like `https://gitlab.com/gitlab-org/cli`, this will
  just work. If you just pass a project name, like `gitlab-org/cli`, then you will also need to pass
  `--forge gitlab` in order to tell `ubi` to use gitlab. The library interface works the same way,
  with a new `UbiBuilder::forge` method. Requested by @SangeloDev. GH #51.
- When looking for macOS assets, `ubi` will now match against `macosx` in asset names, not just
  `macos` and `osx`. Implemented by @kattouf (Vasiliy Kattouf). GH #80.
- Added a new `--api-url-base` CLI argument. This should allow you to use `ubi` with Enterprise
  installations of GitHub and GitLab. Requested by @oalders (Olaf Alders). GH #69.
- Renamed the `UbiBuilder::url_base` method to `api_base_url` and changed it to take a `&str`
  instead of a `String`, which is consistent with all the other builder methods.

## 0.2.4 - 2024-11-24

- `ubi` will now look for just "mac" or "Mac" in a filename when running on macOS. Previously, `ubi`
  would not treat a filename like "foo-mac-x86-64.tar.gz" as a match for macOS. Reported by @jdx
  (Jeff Dickey). GH #79.

## 0.2.3 - 2024-11-22

- Fixed the code to detect whether the filename includes a version number that looks like an
  extension. If the filename ended with the version, like `foo-1.2.3`, then this was not handled
  properly. It only worked if there something after the version as well, like
  `foo-1.2.3-linux-amd64`. Based on a bug report by @FelisNivalis. GH #77.

## 0.2.2 - 2024-11-10

- Added an `is_musl` method to the `UbiBuilder` struct to allow setting this manually.
- Fix handling of file "extensions" that just contain the OS and architecture, like ".linux.amd64".
  Implemented by @jdx (Jeff Dickey). GH #71.

## 0.2.1 - 2024-10-27

- When running on Linux, `ubi` now checks to see if the platform is using `musl` and will prefer a
  release artifact with "musl" in the name. Previously, it would usually pick a glibc artifact if
  there were multiple artifacts that matched the platform OS and architecture, which would not work
  on musl-based platforms like Alpine Linux. Reported by @Burner. GH #70.
- Fixed a bug in the handling of release artifact names with version numbers in them that look like
  extensions. This caused `ubi` to fail when trying to install `shfmt` 3.10.0, and probably many
  other tools. Reported by @jimeh (Jim Myhrberg). GH #67.
- Work around release artifacts that put the platform name after a period, so it looks like an
  extension. This is the case with `direnv`, at least with the v2.35.0 release, which has releast
  artifacts like "direnv.linux-amd64". Reported by @jimeh (Jim Myhrberg). GH #67.

## 0.2.0 - 2024-09-02

- For this release, the library and CLI code have been split into two crates. The library code now
  has fewer dependencies, as there were a few dependencies that were only needed for the CLI code,
  notably `clap`, `fern`, and `tokio`.

## 0.1.2 - 2024-08-31

- Added several cargo features to control which crates `reqwest` uses for TLS. The features are:

  - **`rustls-tls`** _(enabled by default)_ — enables the `rustls-tls` feature for the `reqwest`
    crate.
  - **`rustls-tls-native-roots`** — enables the `rustls-tls-native-roots` feature for the `reqwest`
    crate.
  - **`native-tls`** — enables the `native-tls` feature for the `reqwest` crate.
  - **`native-tls-vendored`** — enables the `native-tls-vendored` feature for the `reqwest` crate.

  Requested by @jdx. GH #62.

## 0.1.1 - 2024-07-21

- Fix documentation links to link to the library docs, not the CLI docs.

## 0.1.0 - 2024-07-21

- UBI can now be used as a library. See the [`ubi` docs on docs.rs](https://docs.rs/ubi/latest/ubi/)
  for more details.

## 0.0.32 - 2024-06-01

- Fix support for plain `.tar` files with no compression.
- Fix handling of files with a version in the filename and no extension, like
  `shfmt_v3.8.0_linux_arm64`. This was fixed before but I broke it in the 0.0.31 release.

## 0.0.31 - 2024-06-01

- Added support for the `.bz2` and `.tar.bz2` file extensions.

## 0.0.30 - 2024-05-11

- When a project's releases contain a mix of file names with and without an architecture, `ubi` will
  try one of the no-architecture names if it doesn't find any matches for the current architecture.
  An example of this is the `yt-dlp/yt-dlp` project, which has releases named `yt-dlp_linux` and
  `yt-dlp_linux_aarch64`.
- `ubi` is now always compiled with `rustls`, instead of using `openssl` on some platforms.

## 0.0.29 - 2023-12-17

- If there is only one match for the platform's OS and the release filename has no architecture in
  it, `ubi` will now pick that one (and hope that it works). This fixes an issue reported by
  @krisan. GH #48.
- As of this release there are no longer binaries built for MIPS on Linux. These targets have been
  demoted to tier 3 support by the Rust compiler.

## 0.0.28 - 2023-09-09

- Fixed a bug with tarballs that use the GNU sparse format. Such tarballs were not extracted
  properly, leading to the extracted executable being garbled. This was an issue with the macOS
  x86-64 release of ubi, which broke the `--self-upgrade` flag on that platform. Reported by Olaf
  Alders. GH #45.

## 0.0.27 - 2023-08-19

- The bootstrap script should handle more possible ARM processors correctly, including for the
  Raspberry Pi. Reported by Olaf Alders. GH #42.
- On macOS ARM, ubi will now pick an x86-64 macOS binary if no ARM binary is available. Reported by
  Olaf Alders. GH #44.

## 0.0.26 - 2023-06-03

- The bootstrap script has been updated to try to handle more operating systems and CPU
  architectures. In addition, you can bypass its platform detection entirely by setting a `FILENAME`
  environment variable, which should be the name of one of the
  [release file assets](https://github.com/houseabsolute/ubi/releases). Reported by Ole-Andreas
  Nylund. Addresses GH #38.
- On 32-bit platforms, `ubi` would always fail when given a `--matching` option on the command line.
  Reported by Ole-Andreas Nylund. Fixes #40.

## 0.0.25 - 2023-05-13

- Help output is now line-wrapped based on your terminal width.
- Fix handling of tarballs that contain a directory matching the project name. In such cases, `ubi`
  would extract that directory instead of looking for the binary _in_ the tarball. Reported by
  Rafael Bodill. GH #36.

## 0.0.24 - 2023-04-20

- Fixed a bug when there were multiple potential matching releases for a platform, and either none
  of the releases were 64-bit or the platform itself was not a 64-bit platform.

## 0.0.23 - 2023-04-11

- Fix match for the jq and mkcert projects. This expands the matching a bit on Linux x86 platforms
  to match "linux32" and "linux64". It also handles filenames with version strings like
  "mkcert-v1.4.4-linux-arm" properly. Previously, it treated the last bit after the "." in the
  version as an extension and rejected this as an invalid extension. Now there is a bit of a gross
  hack to check explicitly for versions in the filename that appear to be an extension. Addresses
  #34.

## 0.0.22 - 2023-04-02

- The `--self-upgrade` option now works on Windows. However, it leaves behind a binary named
  `ubi-old.exe` that must be deleted manually. Addresses #21.

## 0.0.21 - 2023-03-12

- Improved matching of OS and CPU architecture names in release asset names. This release should do
  a better job with more projects.

## 0.0.20 - 2023-03-04

- This release includes a number of changes to support building on many more platforms.
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
  - The code supports some other OS and CPU architectures internally, but I do not have any way to
    build these:
    - Fuchsia x86-64 and aarch64 - not supported by `cross`.
    - Illumos x86-64 - OpenSSL build fails with odd error about `granlib` executable.
    - Linux Sparc64 - not supported by OpenSSL.
    - Solaris x86-64 - supported by `cross` but building the [`mio`](https://lib.rs/crates/mio)
      crate fails.
    - Solaris Sparc - not supported by OpenSSL.
  - In order to do this, `ubi` now uses the [`openssl`](https://lib.rs/crates/openssl) crate under
    the hood instead of [`rustls`](https://lib.rs/crates/rustls). That's because `rustls` depends on
    [`ring`](https://lib.rs/crates/ring), which does not support nearly as many CPU architectures as
    OpenSSL. The `vendored` feature for the `openssl` crate is enabled, which causes it to compile
    and statically link a copy of OpenSSL into the resulting binary. This makes the resulting binary
    more portable at the cost of not using the system OpenSSL.

## 0.0.19 - 2023-02-18

- Fixed handling of bare executables on Windows. It would reject these because it wasn't expecting
  to download a file with a `.exe` extension.

## 0.0.18 - 2023-01-22

- Most errors no longer print out usage information. Now this is only printed for errors related to
  invalid CLI arguments. GH #22.
- Really fix handling of bare xz-compressed binaries. Based on PR #27 from Marco Fontani.
- Add support for bare bz-compressed binaries.

## 0.0.17 - 2022-10-29

- Fixed handling of xz-compressed tarballs. These were ignored even though there was code to handle
  them properly. Reported by Danny Kirkham. GH #24.

## 0.0.16 - 2022-10-04

- Fixed matching the "aarch64" architecture for macOS. At least with Go, these binaries end up
  labeled as "arm64" instead of "aarch64", and `ubi` should treat that as a match. Reported by Ajay
  Vijayakumar.

## 0.0.15 - 2022-09-05

- Added a `--self-upgrade` flag, which will use `ubi` to upgrade `ubi`. Note that this flag does not
  work on Windows.

## 0.0.14 - 2022-09-04

- Added a `--url` flag as an alternative to `--project`. This bypasses the need for using the GitHub
  API, so you don't have to worry about the API limits. This is a good choice for use in CI.

## 0.0.13 - 2022-09-01

- Releases are now downloaded using the GitHub REST API instead of trying to just download a tarball
  directly. This lets `ubi` download releases from private projects.

## 0.0.12 - 2022-07-04

- Bare xz-compressed binaries are now handled properly. Previously ubi would download and "install"
  the compressed file as an executable. Now ubi will uncompress this file properly. Based on PR #19
  from Marco Fontani.
- Fixed a bug in handling of xz-compressed tarballs. There was some support for this, but it wasn't
  complete. These should now be handled just like other compressed tarballs.

## 0.0.11 - 2022-07-03

- Improved handling of urls passed to `--project` so any path that contains an org/user and repo
  works. For example `https://github.com/houseabsolute/precious/releases` and
  `https://github.com/BurntSushi/ripgrep/pull/2049` will now work.
- All Linux binaries are now compiled with musl statically linked instead of dynamically linking
  glibc. This should increase portability.
- The Linux ARM target is now just "arm" instead of "armv7", without hard floats ("hf"). This should
  make the ARM binary more portable.

## 0.0.10 - 2022-07-02

- Fixed releases for Linux ARM64 and ARM 32-bit. The 32-bit release was getting uploaded as aarch64.

## 0.0.9 - 2022-07-02

- This release includes native binaries for Linux ARM64 and ARM (32-bit).
- Added a new flag, `--matching`, which can be used to pick a specific release file when there are
  multiple matching options for your OS and CPU architecture. Based on PR #18 from Marco Fontani.
  Fixes #17.
- When there multiple matches and `--matching` is not given, the same release file will always be
  picked. Previously this was not guaranteed.
- Improved filtering of 32-bit executables when running on 64-bit machines.

## 0.0.8 - 2022-04-25

- No code changes from the last release. The binary releases built by GitHub Actions now build on
  Ubuntu 18.04 instead of 20.04. This restores compatibility with systems using glibc 2.27. Reported
  by Olaf Alders. GH #16.
- This release also includes native ARM64 binaries for macOS 11+.

## 0.0.7 - 2022-04-23

- Include "x64" as a match for the `x86_64` architecture.

## 0.0.6 - 2021-01-15

- Changed CPU architecture matching to be stricter based on the current platform's CPU.
- Changed file extension mapping to work of an allowed list of extensions. This is stricter than the
  previous check, which just filtered out a few things like `.deb` and `.rpm`.

## 0.0.5 - 2021-01-15

- Include s390 and s390x in possible arch list. This also fixes a bug where that arch might be used
  when running `ubi` on any platform.
- Ignore `.deb` and `.rpm` files.
- Look for multiple valid files to download and prefer 64-bit binaries on 64-bit CPUs.

## 0.0.4 - 2021-01-15

- Add support for releases which are either the bare executable or a gzipped executable, like
  rust-analyzer.

## 0.0.3 - 2021-01-15

- Update tokio and other async deps to avoid panics and eliminate deprecated net2 crate from dep
  tree.

## 0.0.2 - 2021-01-09

- When running on Windows, add ".exe" to the user-supplied --exe name if it doesn't already have it.
  This makes it simpler to use ubi with the exact same invocation across platforms.

## 0.0.1 - 2021-01-07

- First release
