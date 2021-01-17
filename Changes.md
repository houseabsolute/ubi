## 0.0.6

* Changed CPU architecture matching to be stricter based on the current
  platform's CPU.


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
