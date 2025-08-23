use std::sync::LazyLock;
use url::Url;

pub(crate) static PROJECT_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://codeberg.org").unwrap());

pub(crate) static DEFAULT_API_BASE_URL: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://codeberg.org/api/v1").unwrap());
