## 0.0.9

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
