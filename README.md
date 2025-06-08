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

The `ubi` CLI tool takes the following command line flags:

| Key                                       | Type          | Required?                                  | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------------------------- | ------------- | ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-p`, `--project <project>`               | string        | no (but you must pass this or `--url`)     | The project you want to install, like houseabsolute/precious or https://github.com/houseabsolute/precious. You cannot pass this with the `--url` flag.                                                                                                                                                                                                                                                                                                                                                                              |
| `-t`, `--tag <tag>`                       | string        | no                                         | The tag to download. Defaults to the latest release. This is only valid if you also pass `--project`.                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `-u`, `--url <url>`                       | string        | no (but you must pass this or `--project`) | The url of the file to download. This can be provided instead of a project or tag. This will not use the forge site's API, so you will never hit its API limits. With this parameter, you do not need to set a token env var except for private repos. You cannot pass `--project` or `--tag` with this flag.                                                                                                                                                                                                                       |
| `-i`, `--in <in>`                         | string        | no                                         | The directory in which the binary should be placed. Defaults to `./bin`.                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `-e`, `--exe <exe>`                       | string        | no                                         | The name of the file to look for in an archive file, or the name of the downloadable file excluding its extension, e.g. `ubi.gz`. By default this is the same as the project name, so for houseabsolute/precious we look for precious or precious.exe. When running on Windows the `.exe` suffix will be added, as needed. You cannot pass `--extract-all` when this is set.                                                                                                                                                        |
| `-m`, `--matching <matching>`             | string        | no                                         | A string that will be matched against the release filename when there are multiple matching files for your OS/arch. For example, there may be multiple releases for an OS/arch that differ by compiler (MSVC vs. gcc) or linked libc (glibc vs. musl). Note that this will be ignored if there is only one matching release filename for your OS/arch.                                                                                                                                                                              |
| `-r`, `--matching-regex <matching-regex>` | string        | no                                         | A regular expression string that will be matched against release filenames before matching against your OS/arch. If the pattern yields a single match, that release will be selected. If no matches are found, this will result in an error.                                                                                                                                                                                                                                                                                        |
| `--rename-exe <rename-exe-to>`            | string        | no                                         | The name to use for the executable after it is unpacked. By default this is the same as the name of the file passed for the `--exe` flag. If that flag isn't passed, this is the same as the name of the project. Note that when set, this name is used as-is, so on Windows, `.exe` will not be appended to the name given. You cannot pass `--extract-all` when this is set.                                                                                                                                                      |
| `--extract-all`                           | boolean       | no                                         | Pass this to tell `ubi` to extract all files from the archive. By default `ubi` will only extract an executable from an archive file. But if this is true, it will simply unpack the archive file. If all of the contents of the archive file share a top-level directory, that directory will be removed during unpacking. In other words, if an archive contains `./project/some-file` and `./project/docs.md`, it will extract them as `some-file` and `docs.md`. You cannot pass `--exe` or `--rename-exe-to` when this is set. |
| `--forge <forge>`                         | enum (string) | no                                         | The forge to use. If this isn't set, then the value of `--project` or `--url` will be checked for gitlab.com. If this contains any other domain _or_ if it does not have a domain at all, then the default is GitHub. \[possible values: `github`, `gitlab`\]                                                                                                                                                                                                                                                                       |
| `--api-base-url <api-base-url>`           | string        | no                                         | The base URL for the forge site's API. This is useful for testing or if you want to operate against an Enterprise version of GitHub or GitLab. This should be something like `https://github.my-corp.example.com/api/v4`.                                                                                                                                                                                                                                                                                                           |
| `--self-upgrade`                          | boolean       | no                                         | Use ubi to upgrade to the latest version of ubi. The `--exe`, `--in`, `--project`, `--tag`, and `--url` args will be ignored.                                                                                                                                                                                                                                                                                                                                                                                                       |
| `-v`, `--verbose`                         | boolean       | no                                         | Enable verbose output.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `-d`, `--debug`                           | boolean       | no                                         | Enable debugging output.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `-q`, `--quiet`                           | boolean       | no                                         | Suppresses most output.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `-h`, `--help`                            | bool,ean      | no                                         | Print help.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `-V`, `--version`                         | boolean       | no                                         | Print version                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |

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
the same name as the project. In either case, the file will be installed with the name it has in the
archive file.

If the release is in the form of a bare executable or a compressed executable, then the installed
executable will use the name of the project instead.

This is a bit inconsistent, but it's how `ubi` has behaved since it was created, and I find this to
be the sanest behavior. Some projects, for example `rust-analyzer`, provide releases as compressed
executables with names like `rust-analyzer-x86_64-apple-darwin` and
`rust-analyzer-x86_64-unknown-linux-musl`, so installing these as `rust-analyzer` seems like better
behavior.

## How `ubi` Finds the Right Release Artifact

<!-- prettier-ignore-start -->
> [!WARNING]
> Note that the exact set of steps that `ubi` follows to find a release artifacts is not considered
> part of the API, and may change in any future release.
<!-- prettier-ignore-end -->

When `ubi` looks at the release assets (downloadable files) for a project, it tries to find the
"right" asset for the platform it's running on. The matching logic currently works like this:

First it filters out assets with extensions it doesn't recognize. Right now this is anything that
doesn't match one of the following:

- `.AppImage` (Linux only)
- `.bat` (Windows only)
- `.bz`
- `.bz2`
- `.exe` (Windows only)
- `.gz`
- `.jar`
- `.pyz`
- `.tar`
- `.tar.bz`
- `.tar.bz2`
- `.tar.gz`
- `.tar.xz`
- `.tbz`
- `.tgz`
- `.txz`
- `.xz`
- `.zip`
- No extension

It tries to be careful about what constitutes an extension. It's common for release filenames to
include a dot (`.`) in the filename before something that's _not_ intended as an extension, for
example `some-tool.linux.amd64`.

If, after filtering for extensions, there's only one asset, it will try to install this one, on the
assumption that this project releases assets which are not platform-specific (like a shell script)
_or_ that this project only releases for one platform and you're running `ubi` on that platform.

If there are multiple matching assets, it will first filter them based on your platform. It does
this in several stages:

- First it filters based on your OS, which is something like Linux, macOS, Windows, FreeBSD, etc. It
  looks at the asset filenames to see which ones match your OS, using a (hopefully complete) regex.
- Next it filters based on your CPU architecture, which is something like x86-64, ARM64, PowerPC,
  etc. Again, this is done with a regex.
- If you are running on a Linux system using musl as its libc, it will also filter out anything
  _not_ compiled against musl. This filter looks to see if the file name contains an indication of
  which libc it was compiled against. Typically, this is something like "-gnu" or "-musl". If it
  does contain this indicator, names that are _not_ musl are filtered out. However, if there is no
  libc indicator, the asset will still be included.

At this point, any remaining assets should work on your platform, so if there's more than one match,
it attempts to pick the best one.

- If it finds both 64-bit and 32-bit assets and you are on a 64-bit platform, it filters out the
  32-bit assets.
- If you've provided a `--matching` string, this is used as a filter at this point.
- If your platform is macOS on ARM64 and there are assets for both x86-64 and ARM64, it filters out
  the non-ARM64 assets.

Finally, if there are still multiple assets left, it sorts them by file name and picks the first
one. The sorting is done to make sure it always picks the same one every time it's run.

## How `ubi` Finds the Right Executable in an Archive File

If the selected release artifact is an archive file (a tarball or zip file), then `ubi` will look
inside the archive to find the right executable.

It first tries to find a file matching the exact name of the project (plus an extension on Windows).
So for example, if you're installing
[`houseabsolute/precious`](https://github.com/houseabsolute/precious), it will look in the archive
for a file named `precious` on Unix-like systems and `precious.bat` or `precious.exe` on Windows.
Note that if it finds an exact match, it does not check the file's mode.

If it can't find an exact match it will look for a file that _starts with_ the project name. This is
mostly to account for projects that include things like platforms or release names in their
executables. Using [`houseabsolute/precious`](https://github.com/houseabsolute/precious) as an
example again, it will match a file named `precious-linux-amd64` or `precious-v1.2.3`. In this case,
it will _rename_ the extracted file to `precious`. On Unix-like systems, these partial matches will
only be considered if the file's mode includes an executable bit. On Windows, it looks for a partial
match that is a `.bat` or `.exe` file, and the extracted file will be renamed to `precious.bat` or
`precious.exe`.

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

Similarly, the GitLab CI system sets a `CI_JOB_TOKEN` for all jobs. Make sure this environment
variable is set when you use `ubi` to install something from GitLab in CI.

If you only run `ubi` on one platform, you can avoid hitting the GitHub or GitLab API entirely by
using the `--url` parameter. But if you run on multiple platforms this can be tedious to maintain
and it largely defeats the purpose of using `ubi`.

If you are downloading executables from repos you don't control _and_ you don't use the `--url`
parameter, then you should use the `--tag` parameter to specify the released version you want to
install. Otherwise `ubi` will always download the latest version, which can lead to surprises,
especially if you are running the tools you download in CI.

## Using `ubi` with GitHub Enterprise or GitLab for Enterprise

The command line tool takes an `--api-base-url` flag for this purpose. This should be the full URL
to the root of the API, something like `https://github.my-corp.example.com/api/v4`.

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

I think so. While you can use `go` or `cargo` to install these tools, that requires an entire
language toolchain. Then you have to actually compile the tool, which may require downloading and
compiling many dependencies. This is going to be a lot slower and more error prone than installing a
binary.

### Is This Better Than Installing from a deb/RPM/homebrew/chocolatey Package?

That's debatable. The big advantage of using `ubi` is that you can use `ubi` in the same way on
Linux, macOS, and Windows. The big disadvantage is that you're not using a package manager, so you
don't get any record of the installation, a way to uninstall, etc. If a tool provides
platform-specific packages for your platforms, you should probably consider using those instead of
`ubi`.

### Is this Better Than Installing via `curl https://some.site/random/installer.sh | sh`?

Isn't literally anything else better than this?

In all seriousness, `ubi` does not download arbitrary code from a random website and execute it
locally when you install anything. That seems like a good thing.

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
