# The Universal Binary Installer Library and CLI Tool

When I say "universal", I mean it downloads binaries from GitHub or GitLab releases.

When I say "binary", I mean it handles single-file executables like those created by most Go and
Rust projects.

When I say "installer", I mean it plops the binary wherever you tell it to.

And finally, when I say "UBI", I don't mean
"[universal basic income](https://en.wikipedia.org/wiki/Universal_basic_income)", but that'd be nice
too.

## Using UBI as a Library

```
[dependencies]
ubi = "x.y.z"
```

See the [`ubi` docs on docs.rs](https://docs.rs/ubi/latest/ubi/) for more details.

## Installing the CLI Tool

You can install the CLI tool by hand by downloading the latest
[release from the releases page](https://github.com/houseabsolute/ubi/releases).

There are also bootstrap installer scripts that provide a half-assed implementation of `ubi`:

### Linux, macOS, FreeBSD, and NetBSD

```
curl --silent --location \
    https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
    sh
```

If you run this as a non-root user, it will install `ubi` into `$HOME/bin`. If run as root it
installs it into `/usr/local/bin`.

#### Environment Variable Parameters

The bootstrap script supports several environment variables as parameters.

| Variable       | Description                                                                                                                                                                                                                                                                                               |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TARGET`       | The directory in which to install `ubi`. Defaults to `$HOME/bin` for non-root users and `/usr/local/bin` for root.                                                                                                                                                                                        |
| `TAG`          | The `ubi` version tag to download. Defaults to the latest release.                                                                                                                                                                                                                                        |
| `FILENAME`     | The name of the [release file asset](https://github.com/houseabsolute/ubi/releases) to download. This skips the platform detection and just downloads the file with this name. Use this if the bootstrap script fails to detect your platform (but please consider submitting a PR to fix the detection). |
| `GITHUB_TOKEN` | The GitHub API token to use when downloading releases. This is only necessary for private repos or if you are hitting the GitHub API anonymous usage limits. Hitting these limits is mostly likely to happen when you're running the bootstrap script repeatedly in CI.                                   |

To set these variables, you can either set them in the environment before running the script, or you
can set them on the command line. Note that you need to set them on the _right_ side of the pipe.
For example, to install a specific version of `ubi` using the `TAG` env var:

```
curl --silent --location \
    https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
    TAG=v0.0.15 sh
```

**Note for GitHub Enterprise:** If you are running this script from an Action in a GitHub Enteprise
installation, the `GITHUB_TOKEN` environment variable will be for that GH Enterprise setup. You will
need to create a separate token for github.com, and explicitly pass that as your `GITHUB_TOKEN`.

### Windows

```
powershell -exec bypass -c "Invoke-WebRequest -URI 'https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.ps1' -UseBasicParsing | Invoke-Expression"
```

You can run this from a command or the Powershell command line. This will install `ubi.exe` into the
current directory.

## How to Use It

```
Usage: ubi [OPTIONS]

Options:
  -p, --project <project>    The project you want to install, like houseabsolute/precious or
                             https://github.com/houseabsolute/precious.
  -t, --tag <tag>            The tag to download. Defaults to the latest release.
  -u, --url <url>            The url of the file to download. This can be provided instead of a
                             project or tag. This will not use the forge site's API, so you will
                             never hit its API limits. With this parameter, you do not need to set a
                             token env var except for private repos.
      --self-upgrade         Use ubi to upgrade to the latest version of ubi. The --exe, --in,
                             --project, --tag, and --url args will be ignored.
  -i, --in <in>              The directory in which the binary should be placed. Defaults to ./bin.
  -e, --exe <exe>            The name of this project's executable. By default, this is the same as
                             the project name, but case-insensitive. For example, with a project
                             named `houseabsolute/precious` it looks for `precious`, `precious.exe`,
                             `Precious`, `PRECIOUS.exe`, etc. When running on Windows the ".exe"
                             suffix will be added as needed, so you should never include this in the
                             value passed to `exe`.
  -m, --matching <matching>  A string that will be matched against the release filename when there
                             are multiple matching files for your OS/arch. For example, there may be
                             multiple releases for an OS/arch that differ by compiler (MSVC vs. gcc)
                             or linked libc (glibc vs. musl). Note that this will be ignored if
                             there is only one matching release filename for your OS/arch.
      --forge <forge>        The forge to use. If this isn't set, then the value of --project or
                             --url will be checked for gitlab.com. If this contains any other domain
                             _or_ if it does not have a domain at all, then the default is GitHub.
                             [possible values: github, gitlab]
  -v, --verbose              Enable verbose output.
  -d, --debug                Enable debugging output.
  -q, --quiet                Suppresses most output.
  -h, --help                 Print help
  -V, --version              Print version
```

## Using a Forge Token

You can set a token for GitHub in the `GITHUB_TOKEN` environment variable. For GitLab, you can
either use `CI_JOB_TOKEN` or `GITLAB_TOKEN`. The former is set in GitLab CI automatically, and it
will be preferred if both are set.

If a token environment variable is set, then this will be used for all API calls. This is required
to download releases for a private project. If you are running `ubi` against GitHub in a CI
environment that runs jobs frequently, you may also need this, as GitHub has a very low rate limit
for anonymous API requests.

However, you can also use the `--url` option to bypass the forge site API by providing the download
link directly.

## Installed Executable Naming

If the release is in the form of a tarball or zip file, `ubi` will look in that archive file for a
file that matches the value given for the `exe` field, if any. Otherwise it looks for a file with
the same name as the project. In either case, does case-insensitive matching/

The file it matches will be installed with whatever casing it has in the archive file. So if a
project is named "SomeProject" and it releases a tarball that contains a "someproject" executable,
`ubi` will find it and install it with that name.

If the release is in the form of a bare executable or a compressed executable, then the installed
executable will use the name of the project instead.

This is a bit inconsistent, but it's how `ubi` has behaved since it was created, and I find this to
be the sanest behavior. Some projects, for example `rust-analyzer`, provide releases as compressed
executables with names like `rust-analyzer-x86_64-apple-darwin.gz` and
`rust-analyzer-x86_64-unknown-linux-musl.gz`, so installing these as `rust-analyzer` seems like
better behavior.

## Upgrading `ubi`

You can run `ubi --self-upgrade` to upgrade `ubi` using `ubi`. Note that you must have write
permissions to the directory containing `ubi` for this to work.

On Windows, this leaves behind a file named `ubi-old.exe` that must be deleted manually.

## Best Practices for Using `ubi` in CI

There are a few things you'll want to consider when using `ubi` in CI.

First, there are forge site API rate limits. See the
[GitHub API rate limits documentation](https://docs.github.com/en/rest/overview/resources-in-the-rest-api#rate-limiting)
and
[GitLab API rate limits documentation](https://docs.gitlab.com/ee/user/gitlab_com/index.html#gitlabcom-specific-rate-limits).

The GitHub limit can be as low as 60 requests per hour per IP when not providing a `GITHUB_TOKEN`,
so you will almost certainly want to provide this if you are getting releases from GitHub.

When running in GitHub Actions you can use the `${{ secrets.GITHUB_TOKEN }}` syntax to set this env
var, and in that case the rate limits are per repository.

```yaml
- name: Install UBI
  shell: bash
  run: |
    curl --silent --location \
        https://raw.githubusercontent.com/houseabsolute/ubi/master/bootstrap/bootstrap-ubi.sh |
        sh
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

- name: Install tools with UBI
  shell: bash
  run: |
    "$HOME/bin/ubi" --project houseabsolute/precious --in "$HOME/bin"
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

Similarly, the GitLab CI system sets a `CI_JOB_TOKEN` for all jobs. Make sure to pass this to UBI
when you use it to install something from GitLab in CI.

If you only run `ubi` on one platform, you can avoid hitting the GitHub API entirely by using the
`--url` parameter. But if you run on multiple platforms this can be tedious to maintain and it
largely defeats the purpose of using `ubi`.

If you are downloading executables from repos you don't control _and_ you don't use the `--url`
parameter, then you should use the `--tag` parameter to specify the released version you want to
install. Otherwise `ubi` will always download the latest version, which can lead to surprises,
especially if you are running the tools you download in CI.

## Why This Is Useful

With the rise of Go and Rust, it has become increasingly common for very useful tools like
[ripgrep](https://github.com/BurntSushi/ripgrep) to publish releases in the form of a tarball or zip
file containing a single executable. Having a single tool capable of downloading the right binary
for your platform is quite handy.

Yes, this can be done in half a dozen lines of shell on Unix systems, but do you know how to do the
equivalent in Powershell?

Once you have `ubi` installed, you can use it to install any of these single-binary tools on Linux,
macOS, and Windows.

### Is This Better Than Installing from Source?

I think so. While you can of course use `go` or `cargo` to install these tools, that requires an
entire language toolchain. Then you have to actually compile the tool, which may require downloading
and compiling many dependencies. This is going to be a lot slower and more error prone than
installing a binary.

### Is This Better Than Installing from a deb/RPM/homebrew/chocolatey Package?

That's debatable. The big advantage of using `ubi` is that you can use the exact same tool on Linux,
macOS, and Windows. The big disadvantage is that you don't get a full package that contains metadata
(like a license file) or extras like shell completion files, nor can you easily uninstall it using a
package manager.

## Linting and Tidying this Code

The code in this repo is linted and tidied with
[`precious`](https://github.com/houseabsolute/precious). This repo contains a `mise.toml` file.
[Mise](https://mise.jdx.dev/) is a tool for managing dev tools with per-repo configuration. You can
install `mise` and use it to run `precious` as follows:

```
# Installs mise
curl https://mise.run | sh
# Installs precious and other dev tools
mise install
```

Once this is done, you can run `precious` via `mise`:

```
# Lints all code
mise exec -- precious lint -a
# Tidies all code
mise exec -- precious tidy -a
```

If you want to use `mise` for other projects, see [its documentation](https://mise.jdx.dev/) for
more details on how you can configure your shell to always activate `mise`.
