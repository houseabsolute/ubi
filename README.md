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

There are also bootstrap install scripts that provide a half-assed
implementation of `ubi`:

### Linux and macOS

```
curl --silent --location \
    https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
    sh
```

If you run this as a non-root user, it will install `ubi` into `$HOME/bin`. If
run as root it installs it into `/usr/local/bin`.

To install `ubi` into an arbitrary location, set the `$TARGET` env var:

```
curl --silent --location \
    https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
    TARGET=~/local/bin sh
```

If the `GITHUB_TOKEN` env var is set, then the bootstrap script will use this
when it hits the GitHub API. This is only necessary if you are hitting the
GitHub anonymous API usage limits. This is unlikely to happen unless you're
running the bootstrap script repeatedly for testing.

### Windows

```
powershell -exec bypass -c "Invoke-WebRequest -URI 'https://raw.githubusercontent.com/houseabsolute/ubi/ci-for-bootstrap/bootstrap/bootstrap-ubi.ps1' -UseBasicParsing | Invoke-Expression"
```

You can run this from a command or Powershell command line. This will install
`ubi.exe` into the directory where you run this.

## How to Use It

```
USAGE:
    ubi [OPTIONS]

OPTIONS:
    -d, --debug                  Enable debugging output
    -e, --exe <exe>              The name of this project's executable. By default this is the same
                                 as the project name, so for houseabsolute/precious we look for
                                 precious or precious.exe. When running on Windows the ".exe" suffix
                                 will be added as needed.
    -h, --help                   Print help information
    -i, --in <in>                The directory in which the binary should be placed. Defaults to
                                 ./bin.
    -m, --matching <matching>    A string that will be matched against the release filename when
                                 there are multiple files for your OS/arch, i.e. "gnu" or "musl".
                                 Note that this will be ignored if there is only used when there is
                                 only one matching release filename for your OS/arch
    -p, --project <project>      The project you want to install, like houseabsolute/precious or
                                 https://github.com/houseabsolute/precious.
    -q, --quiet                  Suppresses most output
    -t, --tag <tag>              The tag to download. Defaults to the latest release.
    -u, --url <url>              The url of the file to download. This can be provided instead of a
                                 project or tag. This will not use the GitHub API, so you will never
                                 hit the GitHub API limits. This means you do not need to set a
                                 GITHUB_TOKEN env var except for private repos.
    -v, --verbose                Enable verbose output
    -V, --version                Print version information
```

## Using a GitHub Token

If the `GITHUB_TOKEN` environment variable is set, then this will be used for
all API calls. This is required to download releases for a private project. If
you are running `ubi` in a CI environment that runs jobs frequently, you may
also need this, as GitHub has a very low rate limit for anonymous API
requests.

However, you can also use the `--url` option to bypass the GitHub API by
providing the download link directly.

## Why This Is Useful

With the rise of Go and Rust, it has become increasingly common for very
useful tools like [ripgrep](https://github.com/BurntSushi/ripgrep) to publish
releases in the form of a tarball or zip file containing a single
executable. Having a single tool capable of downloading the right binary for
your platform is quite handy.

Yes, this can be done in half a dozen lines of shell on Unix systems, but do
you know how to do the equivalent in Powershell?

Once you have `ubi` installed, you can use it to install any of these
single-binary tools available on GitHub, on Linux, macOS, and Windows.

### Is This Better Than Installing from Source?

I think so. While you can of course use `go` or `cargo` to install these
tools, that requires an entire language toolchain. Then you have to actually
compile the tool, which may require downloading and compiling many
dependencies. This is going to be a lot slower and more error prone than
installing a binary.

### Is This Better Than Installing from a deb/RPM/homebrew/chocolatey Package?

That's debatable. The big advantage of using `ubi` is that you can use the
exact same tool on Linux, macOS, and Windows. The big disadvantage is that you
don't get a full package that contains metadata (like a license file) or
extras like shell completion files, nor can you easily uninstall it using a
package manager.
