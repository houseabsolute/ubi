# The Universal Binary Installer

When I say "universal", I mean it downloads binaries from GitHub releases.

When I say "binary", I mean it handles single-file executables like those
created by most Go and Rust projects.

When I say "installer", I mean it plops the binary wherever you tell it to.

And finally, when I say "UBI", I don't mean "[universal basic
income](https://en.wikipedia.org/wiki/Universal_basic_income)", but that'd be
nice too.

## Installing It

You can install it by hand by downloading the latest [release from the
releases page](https://github.com/houseabsolute/ubi/releases).

There are also bootstrap install scripts (well, one script for Unix systems so
far) that provide a half-assed implementation of `ubi`:

### Linux and macOS

```
$> curl --silent --location \
       https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
       sh
```

If you run this as a non-root user, it will install `ubi` into `$HOME/bin`. If
run as root it installs it into `/usr/local/bin`.

### Windows

[I need some help with writing a Powershell
script](https://github.com/houseabsolute/ubi/issues/1).

## How to Use It

```
USAGE:
    ubi [FLAGS] [OPTIONS] --project <project>

FLAGS:
    -d, --debug      Enable debugging output
    -h, --help       Prints help information
    -q, --quiet      Suppresses most output
    -v, --verbose    Enable verbose output
    -V, --version    Prints version information

OPTIONS:
    -e, --exe <exe>            The name of this project's executable. By default this is the same as
                               the project name, so for houseabsolute/precious we look for precious
                               or precious.exe. When running on Windows the ".exe" suffix will be
                               addedas needed.
    -i, --in <in>              The directory in which the binary should be placed. Defaults to
                               ./bin.
    -p, --project <project>    The project you want to install, like houseabsolute/precious or
                               https://github.com/houseabsolute/precious.
    -t, --tag <tag>            The tag to download. Defaults to the latest release.
```

## Using a GitHub Token

If the `GITHUB_TOKEN` environment variable is set, then this will be used for
all API calls. You will almost certainly need to do if you are using `ubi` in
a CI environment that runs jobs frequently, as GitHub has a very low rate
limit for anonymous API requests.

## Why This Is Useful

With the rise of Go and Rust, it has become increasingly common for very
useful tools like [ripgrep](https://github.com/BurntSushi/ripgrep) to publish
releases in the form of a tarball or zip file containing a single
executable. Having a single tool capable of downloading the right binary for
your platform is quite handy.

Yes, this can be done in half a dozen lines of shell on Unix systems, but do
you know how to do the equivalent in Powershell? I don't. Seriously, I could
use [some help with this](https://github.com/houseabsolute/ubi/issues/1).

Once you have `ubi` installed, you can use it to install any of these many
single-binary tools available on GitHub, on any supported platform.

### Is This Better Than Installing from Source?

I think so. While you can of course use `go` or `cargo` to install these
tools, that requires an entire language toolchain. Then you have to actually
compile the tool, which may require many downloading and compiling
dependencies. This is going to be a lot slower and more error prone than
installing a binary.

### Is This Better Than Installing from a deb/RPM/homebrew/chocolatey Package?

That's debatable. The big advantage of using `ubi` is that you can use the
exact same tool on many platforms. The big disadvantage is that you don't get
a full package that contains metadata (like a license file) or extras like
shell completion files, nor can you easily uninstall it using a package
manager.

And of course, not every tool has packages for every platform.
