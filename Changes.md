## 0.0.12 - 2022-07-04

* Handle bare xz-compressed binaries. Previously ubi would download and
  "install" the compressed file as an executable. Now ubi will uncompress this
  file properly. Based on PR #19 from Marco Fontani.
* Fixed a bug in handling of xz-compressed tarballs. There was some support
  for this, but it wasn't complete. These should now be handled just like
  other compressed tarballs.


## 0.0.11 - 2022-07-03

* Improved handling of urls passed to `--project` so any path that contains an
  org/user and repo works. For example
  `https://github.com/houseabsolute/precious/releases` and
  `https://github.com/BurntSushi/ripgrep/pull/2049` will now work.
* All Linux binaries are now compiled with musl statically linked instead of
  dynamically linking glibc. This should increase portability.
* The Linux ARM target is now just "arm" instead of "armv7", without hard
  floats ("hf"). This should make the ARM binary more portable.


## 0.0.10 - 2022-07-02

* Fixed releases for Linux ARM64 and ARM 32-bit. The 32-bit release was
  getting uploaded as aarch64.


## 0.0.9 - 2022-07-02

* This release includes native binaries for Linux ARM64 and ARM (32-bit).
* Added a new flag, `--matching`, which can be used to pick a specific release
  file when there are multiple matching options for your OS and CPU
  architecture. Based on PR #18 from Marco Fontani. Fixes #17.
* When there multiple matches and `--matching` is not given, the same release
  file will always be picked. Previously this was not guaranteed.
* Improved filtering of 32-bit executables when running on 64-bit machines.


## 0.0.8 - 2022-04-25

* No code changes from the last release. The binary releases built by GitHub
  Actions now build on Ubuntu 18.04 instead of 20.04. This restores
  compatibility with systems using glibc 2.27. Reported by Olaf Alders. GH
  #16.
* This release also includes native ARM64 binaries for macOS 11+.


## 0.0.7 - 2022-04-23

* Include "x64" as a match for the `x86_64` architecture.


## 0.0.6 - 2021-01-15

* Changed CPU architecture matching to be stricter based on the current
  platform's CPU.
* Changed file extension mapping to work of an allowed list of
  extensions. This is stricter than the previous check, which just filtered
  out a few things like `.deb` and `.rpm`.


## 0.0.5 - 2021-01-15

* Include s390 and s390x in possible arch list. This also fixes a bug where
  that arch might be used when running `ubi` on any platform.
* Ignore `.deb` and `.rpm` files.
* Look for multiple valid files to download and prefer 64-bit binaries on
  64-bit CPUs.


## 0.0.4 - 2021-01-15

* Add support for releases which are either the bare executable or a gzipped
  executable, like rust-analyzer.


## 0.0.3 - 2021-01-15

* Update tokio and other async deps to avoid panics and eliminate deprecated
  net2 crate from dep tree.


## 0.0.2 - 2021-01-09

* When running on Windows, add ".exe" to the user-supplied --exe name if it
  doesn't already have it. This makes it simpler to use ubi with the exact
  same invocation across platforms.


## 0.0.1 - 2021-01-07

* First release
