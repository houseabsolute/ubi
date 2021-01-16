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
