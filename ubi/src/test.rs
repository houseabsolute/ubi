use crate::UbiBuilder;
use anyhow::Result;
use mockito::Server;
use platforms::PlatformReq;
use reqwest::header::ACCEPT;
use std::str::FromStr;
use test_log::test;
use url::Url;

#[test(tokio::test)]
#[allow(clippy::too_many_lines)]
async fn asset_picking() -> Result<()> {
    struct Test {
        platforms: &'static [&'static str],
        expect_ubi: Option<(u32, &'static str)>,
        expect_omegasort: Option<(u32, &'static str)>,
    }
    let tests: &[Test] = &[
        Test {
            platforms: &["aarch64-apple-darwin"],
            expect_ubi: Some((96_252_654, "ubi-Darwin-aarch64.tar.gz")),
            expect_omegasort: Some((84_376_701, "omegasort_0.0.7_Darwin_arm64.tar.gz")),
        },
        Test {
            platforms: &["x86_64-apple-darwin"],
            expect_ubi: Some((96_252_671, "ubi-Darwin-x86_64.tar.gz")),
            expect_omegasort: Some((84_376_694, "omegasort_0.0.7_Darwin_x86_64.tar.gz")),
        },
        Test {
            platforms: &["x86_64-unknown-freebsd"],
            expect_ubi: Some((1, "ubi-FreeBSD-x86_64.tar.gz")),
            expect_omegasort: Some((84_376_692, "omegasort_0.0.7_FreeBSD_x86_64.tar.gz")),
        },
        Test {
            platforms: &["x86_64-unknown-illumos"],
            expect_ubi: Some((4, "ubi-Illumos-x86_64.tar.gz")),
            expect_omegasort: Some((4, "omegasort_0.0.7_Illumos_x86_64.tar.gz")),
        },
        Test {
            platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
            expect_ubi: Some((96_252_412, "ubi-Linux-aarch64-musl.tar.gz")),
            expect_omegasort: Some((84_376_697, "omegasort_0.0.7_Linux_arm64.tar.gz")),
        },
        Test {
            platforms: &["arm-unknown-linux-musleabi"],
            expect_ubi: Some((96_252_419, "ubi-Linux-arm-musl.tar.gz")),
            expect_omegasort: Some((42, "omegasort_0.0.7_Linux_arm.tar.gz")),
        },
        Test {
            platforms: &[
                "i586-unknown-linux-gnu",
                "i586-unknown-linux-musl",
                "i686-unknown-linux-gnu",
                "i686-unknown-linux-musl",
            ],
            expect_ubi: Some((62, "ubi-Linux-i586-musl.tar.gz")),
            expect_omegasort: Some((62, "omegasort_0.0.7_Linux_386.tar.gz")),
        },
        Test {
            platforms: &["mips-unknown-linux-gnu", "mips-unknown-linux-musl"],
            expect_ubi: Some((50, "ubi-Linux-mips-musl.tar.gz")),
            expect_omegasort: Some((50, "omegasort_0.0.7_Linux_mips.tar.gz")),
        },
        Test {
            platforms: &["mipsel-unknown-linux-gnu", "mipsel-unknown-linux-musl"],
            expect_ubi: Some((52, "ubi-Linux-mipsel-musl.tar.gz")),
            expect_omegasort: Some((52, "omegasort_0.0.7_Linux_mipsle.tar.gz")),
        },
        Test {
            platforms: &[
                "mips64-unknown-linux-gnuabi64",
                "mips64-unknown-linux-muslabi64",
            ],
            expect_ubi: Some((51, "ubi-Linux-mips64-musl.tar.gz")),
            expect_omegasort: Some((51, "omegasort_0.0.7_Linux_mips64.tar.gz")),
        },
        Test {
            platforms: &[
                "mips64el-unknown-linux-gnuabi64",
                "mips64el-unknown-linux-muslabi64",
            ],
            expect_ubi: Some((53, "ubi-Linux-mips64el-musl.tar.gz")),
            expect_omegasort: Some((53, "omegasort_0.0.7_Linux_mips64le.tar.gz")),
        },
        Test {
            platforms: &["powerpc-unknown-linux-gnu"],
            expect_ubi: Some((54, "ubi-Linux-powerpc-gnu.tar.gz")),
            expect_omegasort: Some((54, "omegasort_0.0.7_Linux_ppc.tar.gz")),
        },
        Test {
            platforms: &["powerpc64-unknown-linux-gnu"],
            expect_ubi: Some((55, "ubi-Linux-powerpc64-gnu.tar.gz")),
            expect_omegasort: Some((55, "omegasort_0.0.7_Linux_ppc64.tar.gz")),
        },
        Test {
            platforms: &["powerpc64le-unknown-linux-gnu"],
            expect_ubi: Some((56, "ubi-Linux-powerpc64le-gnu.tar.gz")),
            expect_omegasort: Some((56, "omegasort_0.0.7_Linux_ppc64le.tar.gz")),
        },
        Test {
            platforms: &["riscv64gc-unknown-linux-gnu"],
            expect_ubi: Some((57, "ubi-Linux-riscv64-gnu.tar.gz")),
            expect_omegasort: Some((57, "omegasort_0.0.7_Linux_riscv64.tar.gz")),
        },
        Test {
            platforms: &["s390x-unknown-linux-gnu"],
            expect_ubi: Some((58, "ubi-Linux-s390x-gnu.tar.gz")),
            expect_omegasort: Some((58, "omegasort_0.0.7_Linux_s390x.tar.gz")),
        },
        Test {
            platforms: &["sparc64-unknown-linux-gnu"],
            expect_ubi: Some((59, "ubi-Linux-sparc64-gnu.tar.gz")),
            expect_omegasort: None,
        },
        Test {
            platforms: &["x86_64-unknown-linux-musl"],
            expect_ubi: Some((96_297_448, "ubi-Linux-x86_64-musl.tar.gz")),
            expect_omegasort: Some((84_376_700, "omegasort_0.0.7_Linux_x86_64.tar.gz")),
        },
        Test {
            platforms: &["x86_64-unknown-netbsd"],
            expect_ubi: Some((5, "ubi-NetBSD-x86_64.tar.gz")),
            expect_omegasort: Some((5, "omegasort_0.0.7_NetBSD_x86_64.tar.gz")),
        },
        Test {
            platforms: &["sparcv9-sun-solaris"],
            expect_ubi: Some((61, "ubi-Solaris-sparc64.tar.gz")),
            expect_omegasort: None,
        },
        Test {
            platforms: &["x86_64-pc-solaris"],
            expect_ubi: Some((6, "ubi-Solaris-x86_64.tar.gz")),
            expect_omegasort: Some((6, "omegasort_0.0.7_Solaris_x86_64.tar.gz")),
        },
        Test {
            platforms: &["aarch64-pc-windows-msvc"],
            expect_ubi: Some((7, "ubi-Windows-aarch64.zip")),
            expect_omegasort: Some((84_376_695, "omegasort_0.0.7_Windows_arm64.tar.gz")),
        },
        Test {
            platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
            expect_ubi: Some((96_252_617, "ubi-Windows-x86_64.zip")),
            expect_omegasort: Some((84_376_693, "omegasort_0.0.7_Windows_x86_64.tar.gz")),
        },
    ];

    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/houseabsolute/ubi/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(UBI_LATEST_RESPONSE)
        .expect_at_least(tests.len())
        .create_async()
        .await;
    let m2 = server
        .mock("GET", "/repos/houseabsolute/omegasort/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(OMEGASORT_LATEST_RESPONSE)
        .expect_at_least(tests.len())
        .create_async()
        .await;

    for t in tests {
        for p in t.platforms {
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();

            if let Some(expect_ubi) = t.expect_ubi {
                let mut ubi = UbiBuilder::new()
                    .project("houseabsolute/ubi")
                    .platform(platform)
                    .is_musl(false)
                    .api_base_url(&url)
                    .build()?;
                let asset = ubi.asset().await?;
                let expect_ubi_url = Url::parse(&format!(
                    "https://api.github.com/repos/houseabsolute/ubi/releases/assets/{}",
                    expect_ubi.0
                ))?;
                assert_eq!(
                    asset.url, expect_ubi_url,
                    "picked {expect_ubi_url} as ubi url",
                );
                assert_eq!(
                    asset.name, expect_ubi.1,
                    "picked {} as ubi asset name",
                    expect_ubi.1,
                );
            }

            if let Some(expect_omegasort) = t.expect_omegasort {
                let mut ubi = UbiBuilder::new()
                    .project("houseabsolute/omegasort")
                    .platform(platform)
                    .is_musl(false)
                    .api_base_url(&url)
                    .build()?;
                let asset = ubi.asset().await?;
                let expect_omegasort_url = Url::parse(&format!(
                    "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/{}",
                    expect_omegasort.0
                ))?;
                assert_eq!(
                    asset.url, expect_omegasort_url,
                    "picked {expect_omegasort_url} as omegasort url",
                );
                assert_eq!(
                    asset.name, expect_omegasort.1,
                    "picked {} as omegasort ID",
                    expect_omegasort.1,
                );
            }
        }
    }

    m1.assert_async().await;
    m2.assert_async().await;

    Ok(())
}

// jq '[.assets[] | {"browser_download_url": .url} + {"name": .name}]' release.json
const UBI_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252654",
      "name": "ubi-Darwin-aarch64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252671",
      "name": "ubi-Darwin-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/1",
      "name": "ubi-FreeBSD-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/2",
      "name": "ubi-Fuchsia-aarch64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/3",
      "name": "ubi-Fuchsia-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/4",
      "name": "ubi-Illumos-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252412",
      "name": "ubi-Linux-aarch64-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252419",
      "name": "ubi-Linux-arm-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/62",
      "name": "ubi-Linux-i586-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/50",
      "name": "ubi-Linux-mips-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/52",
      "name": "ubi-Linux-mipsel-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/51",
      "name": "ubi-Linux-mips64-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/53",
      "name": "ubi-Linux-mips64el-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/54",
      "name": "ubi-Linux-powerpc-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/55",
      "name": "ubi-Linux-powerpc64-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/56",
      "name": "ubi-Linux-powerpc64le-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/57",
      "name": "ubi-Linux-riscv64-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/58",
      "name": "ubi-Linux-s390x-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/59",
      "name": "ubi-Linux-sparc64-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96297448",
      "name": "ubi-Linux-x86_64-musl.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/5",
      "name": "ubi-NetBSD-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/61",
      "name": "ubi-Solaris-sparc64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/6",
      "name": "ubi-Solaris-x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/60",
      "name": "ubi-Solaris-sparcv9.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/7",
      "name": "ubi-Windows-aarch64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/ubi/releases/assets/96252617",
      "name": "ubi-Windows-x86_64.zip"
    }
  ]
}
"#;

const OMEGASORT_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376696",
      "name": "checksums.txt"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376701",
      "name": "omegasort_0.0.7_Darwin_arm64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376694",
      "name": "omegasort_0.0.7_Darwin_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376698",
      "name": "omegasort_0.0.7_FreeBSD_arm64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376699",
      "name": "omegasort_0.0.7_FreeBSD_i386.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376692",
      "name": "omegasort_0.0.7_FreeBSD_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/2",
      "name": "omegasort_0.0.7_Fuchsia_arm64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/3",
      "name": "omegasort_0.0.7_Fuchsia_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/4",
      "name": "omegasort_0.0.7_Illumos_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/42",
      "name": "omegasort_0.0.7_Linux_arm.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376697",
      "name": "omegasort_0.0.7_Linux_arm64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/62",
      "name": "omegasort_0.0.7_Linux_386.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/50",
      "name": "omegasort_0.0.7_Linux_mips.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/52",
      "name": "omegasort_0.0.7_Linux_mipsle.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/51",
      "name": "omegasort_0.0.7_Linux_mips64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/53",
      "name": "omegasort_0.0.7_Linux_mips64le.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/54",
      "name": "omegasort_0.0.7_Linux_ppc.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/55",
      "name": "omegasort_0.0.7_Linux_ppc64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/56",
      "name": "omegasort_0.0.7_Linux_ppc64le.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/57",
      "name": "omegasort_0.0.7_Linux_riscv64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/58",
      "name": "omegasort_0.0.7_Linux_s390x.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376703",
      "name": "omegasort_0.0.7_Linux_i386.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376700",
      "name": "omegasort_0.0.7_Linux_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/5",
      "name": "omegasort_0.0.7_NetBSD_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/6",
      "name": "omegasort_0.0.7_Solaris_x86_64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376695",
      "name": "omegasort_0.0.7_Windows_arm64.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376702",
      "name": "omegasort_0.0.7_Windows_i386.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/houseabsolute/omegasort/releases/assets/84376693",
      "name": "omegasort_0.0.7_Windows_x86_64.tar.gz"
    }
  ]
}
"#;

#[test(tokio::test)]
// The protobuf repo has some odd release naming. This tests that the
// matcher handles it.
async fn matching_unusual_names() -> Result<()> {
    struct Test {
        platforms: &'static [&'static str],
        expect: &'static str,
    }
    let tests: &[Test] = &[
        Test {
            platforms: &["aarch64-apple-darwin"],
            expect: "protoc-22.2-osx-aarch_64.zip",
        },
        Test {
            platforms: &["x86_64-apple-darwin"],
            expect: "protoc-22.2-osx-x86_64.zip",
        },
        Test {
            platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
            expect: "protoc-22.2-linux-aarch_64.zip",
        },
        Test {
            platforms: &[
                "i586-unknown-linux-gnu",
                "i586-unknown-linux-musl",
                "i686-unknown-linux-gnu",
                "i686-unknown-linux-musl",
            ],
            expect: "protoc-22.2-linux-x86_32.zip",
        },
        Test {
            platforms: &["powerpc64le-unknown-linux-gnu"],
            expect: "protoc-22.2-linux-ppcle_64.zip",
        },
        Test {
            platforms: &["s390x-unknown-linux-gnu"],
            expect: "protoc-22.2-linux-s390_64.zip",
        },
        Test {
            platforms: &["x86_64-unknown-linux-musl"],
            expect: "protoc-22.2-linux-x86_64.zip",
        },
        Test {
            platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
            expect: "protoc-22.2-win64.zip",
        },
        Test {
            platforms: &["i686-pc-windows-gnu", "i686-pc-windows-msvc"],
            expect: "protoc-22.2-win32.zip",
        },
    ];

    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/protocolbuffers/protobuf/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(PROTOBUF_LATEST_RESPONSE)
        .expect_at_least(tests.len())
        .create_async()
        .await;

    for t in tests {
        for p in t.platforms {
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = UbiBuilder::new()
                .project("protocolbuffers/protobuf")
                .platform(platform)
                .api_base_url(&url)
                .build()?;
            let asset = ubi.asset().await?;
            assert_eq!(
                asset.name, t.expect,
                "picked {} as protobuf asset name",
                t.expect
            );
        }
    }

    m1.assert_async().await;

    Ok(())
}

const PROTOBUF_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875803",
      "name": "protobuf-22.2.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875802",
      "name": "protobuf-22.2.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875801",
      "name": "protoc-22.2-linux-aarch_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875800",
      "name": "protoc-22.2-linux-ppcle_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875799",
      "name": "protoc-22.2-linux-s390_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875810",
      "name": "protoc-22.2-linux-x86_32.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875811",
      "name": "protoc-22.2-linux-x86_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875812",
      "name": "protoc-22.2-osx-aarch_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875813",
      "name": "protoc-22.2-osx-universal_binary.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875814",
      "name": "protoc-22.2-osx-x86_64.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875815",
      "name": "protoc-22.2-win32.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/protocolbuffers/protobuf/releases/assets/98875816",
      "name": "protoc-22.2-win64.zip"
    }
  ]
}
"#;

// Reported in https://github.com/houseabsolute/ubi/issues/34
#[test(tokio::test)]
async fn mkcert_matching() -> Result<()> {
    struct Test {
        platforms: &'static [&'static str],
        expect: &'static str,
    }
    let tests: &[Test] = &[
        Test {
            platforms: &["aarch64-apple-darwin"],
            expect: "mkcert-v1.4.4-darwin-arm64",
        },
        Test {
            platforms: &["x86_64-apple-darwin"],
            expect: "mkcert-v1.4.4-darwin-amd64",
        },
        Test {
            platforms: &["aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl"],
            expect: "mkcert-v1.4.4-linux-arm64",
        },
        Test {
            platforms: &["arm-unknown-linux-gnueabi", "arm-unknown-linux-musleabi"],
            expect: "mkcert-v1.4.4-linux-arm",
        },
        Test {
            platforms: &["x86_64-unknown-linux-musl"],
            expect: "mkcert-v1.4.4-linux-amd64",
        },
        Test {
            platforms: &["x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"],
            expect: "mkcert-v1.4.4-windows-amd64.exe",
        },
    ];

    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/FiloSottile/mkcert/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(MKCERT_LATEST_RESPONSE)
        .expect_at_least(tests.len())
        .create_async()
        .await;

    for t in tests {
        for p in t.platforms {
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = UbiBuilder::new()
                .project("FiloSottile/mkcert")
                .platform(platform)
                .api_base_url(&url)
                .build()?;
            let asset = ubi.asset().await?;
            assert_eq!(
                asset.name, t.expect,
                "picked {} as protobuf asset name",
                t.expect
            );
        }
    }

    m1.assert_async().await;

    Ok(())
}

const MKCERT_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709952",
      "name": "mkcert-v1.4.4-darwin-amd64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709954",
      "name": "mkcert-v1.4.4-darwin-arm64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709955",
      "name": "mkcert-v1.4.4-linux-amd64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709956",
      "name": "mkcert-v1.4.4-linux-arm"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709957",
      "name": "mkcert-v1.4.4-linux-arm64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709958",
      "name": "mkcert-v1.4.4-windows-amd64.exe"
    },
    {
      "browser_download_url": "https://api.github.com/repos/FiloSottile/mkcert/releases/assets/63709963",
      "name": "mkcert-v1.4.4-windows-arm64.exe"
    }
  ]
}"#;

// Reported in https://github.com/houseabsolute/ubi/issues/34
#[test(tokio::test)]
async fn jq_matching() -> Result<()> {
    struct Test {
        platforms: &'static [&'static str],
        expect: &'static str,
    }
    let tests: &[Test] = &[
        Test {
            platforms: &["x86_64-apple-darwin"],
            expect: "jq-osx-amd64",
        },
        Test {
            platforms: &["x86_64-unknown-linux-musl"],
            expect: "jq-linux64",
        },
        Test {
            platforms: &["i686-pc-windows-gnu", "i686-pc-windows-msvc"],
            expect: "jq-win32.exe",
        },
    ];

    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/stedolan/jq/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(JQ_LATEST_RESPONSE)
        .expect_at_least(tests.len())
        .create_async()
        .await;

    for t in tests {
        for p in t.platforms {
            let req = PlatformReq::from_str(p)
                .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
            let platform = req.matching_platforms().next().unwrap();
            let mut ubi = UbiBuilder::new()
                .project("stedolan/jq")
                .platform(platform)
                .api_base_url(&url)
                .build()?;
            let asset = ubi.asset().await?;
            assert_eq!(
                asset.name, t.expect,
                "picked {} as protobuf asset name",
                t.expect
            );
        }
    }

    m1.assert_async().await;

    Ok(())
}

const JQ_LATEST_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9780532",
      "name": "jq-1.6.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9780533",
      "name": "jq-1.6.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521004",
      "name": "jq-linux32"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521005",
      "name": "jq-linux64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521006",
      "name": "jq-osx-amd64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521007",
      "name": "jq-win32.exe"
    },
    {
      "browser_download_url": "https://api.github.com/repos/stedolan/jq/releases/assets/9521008",
      "name": "jq-win64.exe"
    }
  ]
}"#;

#[test(tokio::test)]
async fn multiple_matches() -> Result<()> {
    let platforms = ["x86_64-pc-windows-gnu", "i686-pc-windows-gnu"];

    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/test/multiple-matches/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(MULTIPLE_MATCHES_RESPONSE)
        .expect_at_least(platforms.len())
        .create_async()
        .await;

    for p in platforms {
        let req = PlatformReq::from_str(p)
            .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
        let platform = req.matching_platforms().next().unwrap();
        let mut ubi = UbiBuilder::new()
            .project("test/multiple-matches")
            .platform(platform)
            .api_base_url(&url)
            .build()?;
        let asset = ubi.asset().await?;
        let expect = "mm-i686-pc-windows-gnu.zip";
        assert_eq!(asset.name, expect, "picked {expect} as protobuf asset name");
    }

    m1.assert_async().await;

    Ok(())
}

const MULTIPLE_MATCHES_RESPONSE: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/test/multiple-matches/releases/assets/9521007",
      "name": "mm-i686-pc-windows-gnu.zip"
    },
    {
      "browser_download_url": "https://api.github.com/repos/test/multiple-matches/releases/assets/9521008",
      "name": "mm-i686-pc-windows-msvc.zip"
    }
  ]
}"#;

#[test(tokio::test)]
async fn macos_arm() -> Result<()> {
    let mut server = Server::new_async().await;
    let url = server.url();
    let m1 = server
        .mock("GET", "/repos/test/macos/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(MACOS_RESPONSE1)
        .expect_at_least(1)
        .create_async()
        .await;

    let p = "aarch64-apple-darwin";
    let req = PlatformReq::from_str(p)
        .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
    let platform = req.matching_platforms().next().unwrap();
    let mut ubi = UbiBuilder::new()
        .project("test/macos")
        .platform(platform)
        .api_base_url(&url)
        .build()?;

    {
        let asset = ubi.asset().await?;
        let expect = "bat-v0.23.0-x86_64-apple-darwin.tar.gz";
        assert_eq!(
            asset.name, expect,
            "picked {expect} as macos bat asset name when only x86 binary is available"
        );
        m1.assert_async().await;
    }

    server.reset();

    let m2 = server
        .mock("GET", "/repos/test/macos/releases/latest")
        .match_header(ACCEPT.as_str(), "application/json")
        .with_status(reqwest::StatusCode::OK.as_u16() as usize)
        .with_body(MACOS_RESPONSE2)
        .expect_at_least(1)
        .create_async()
        .await;

    {
        let asset = ubi.asset().await?;
        let expect = "bat-v0.23.0-aarch64-apple-darwin.tar.gz";
        assert_eq!(
            asset.name, expect,
            "picked {expect} as macos bat asset name when ARM binary is available"
        );
        m2.assert_async().await;
    }

    Ok(())
}

const MACOS_RESPONSE1: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "bat-v0.23.0-i686-unknown-linux-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-x86_64-apple-darwin.tar.gz"
    }
  ]
}"#;

const MACOS_RESPONSE2: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "bat-v0.23.0-i686-unknown-linux-gnu.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-x86_64-apple-darwin.tar.gz"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "bat-v0.23.0-aarch64-apple-darwin.tar.gz"
    }
  ]
}"#;

#[test(tokio::test)]
async fn os_without_arch() -> Result<()> {
    {
        let mut server = Server::new_async().await;
        let url = server.url();
        let m1 = server
            .mock("GET", "/repos/test/os-without-arch/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(OS_WITHOUT_ARCH_RESPONSE1)
            .expect_at_least(1)
            .create_async()
            .await;

        let p = "x86_64-apple-darwin";
        let req = PlatformReq::from_str(p)
            .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
        let platform = req.matching_platforms().next().unwrap();
        let mut ubi = UbiBuilder::new()
            .project("test/os-without-arch")
            .platform(platform)
            .api_base_url(&url)
            .build()?;
        let asset = ubi.asset().await?;
        let expect = "gvproxy-darwin";
        assert_eq!(asset.name, expect, "picked {expect} as protobuf asset name");

        m1.assert_async().await;
    }

    {
        let mut server = Server::new_async().await;
        let url = server.url();
        let m1 = server
            .mock("GET", "/repos/test/os-without-arch/releases/latest")
            .match_header(ACCEPT.as_str(), "application/json")
            .with_status(reqwest::StatusCode::OK.as_u16() as usize)
            .with_body(OS_WITHOUT_ARCH_RESPONSE2)
            .expect_at_least(1)
            .create_async()
            .await;

        let p = "x86_64-apple-darwin";
        let req = PlatformReq::from_str(p)
            .unwrap_or_else(|e| panic!("could not create PlatformReq for {p}: {e}"));
        let platform = req.matching_platforms().next().unwrap();
        let mut ubi = UbiBuilder::new()
            .project("test/os-without-arch")
            .platform(platform)
            .api_base_url(&url)
            .build()?;
        let asset = ubi.asset().await;
        assert!(
            asset.is_err(),
            "should not have found an asset because the only darwin asset is for arm64",
        );

        m1.assert_async().await;
    }

    Ok(())
}

const OS_WITHOUT_ARCH_RESPONSE1: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "gvproxy-darwin"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "gvproxy-linux-amd64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891187",
      "name": "gvproxy-linux-arm64"
    }
  ]
}"#;

const OS_WITHOUT_ARCH_RESPONSE2: &str = r#"
{
  "assets": [
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100890821",
      "name": "gvproxy-darwin-arm64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891186",
      "name": "gvproxy-linux-amd64"
    },
    {
      "browser_download_url": "https://api.github.com/repos/sharkdp/bat/releases/assets/100891187",
      "name": "gvproxy-linux-arm64"
    }
  ]
}"#;
