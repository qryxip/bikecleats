macro_rules! lazy_url {
    ($url:literal) => {
        ::once_cell::sync::Lazy::new(|| ::std::primitive::str::parse::<::url::Url>($url).unwrap())
    };
}

macro_rules! static_url {
    ($url:literal) => {{
        static URL: ::once_cell::sync::Lazy<::url::Url> = lazy_url!($url);
        &*URL
    }};
}

macro_rules! url {
    ($fmt:literal $(, $expr:expr)* $(,)*) => {
        // `self::BASE_URL` is defined in each module.

        ::url::Url::join(
            &self::BASE_URL,
            &::std::format!(
                $fmt,
                $(
                    ::percent_encoding::utf8_percent_encode(
                        &::std::string::ToString::to_string(&$expr),
                        ::percent_encoding::NON_ALPHANUMERIC,
                    ),
                )*
            ),
        )
        .unwrap()
    }
}

macro_rules! lazy_selector {
    ($selectors:literal $(,)?) => {
        ::once_cell::sync::Lazy::new(|| ::scraper::Selector::parse($selectors).unwrap())
    };
}

macro_rules! static_selector {
    ($selectors:literal $(,)?) => {{
        static __SELECTOR: ::once_cell::sync::Lazy<::scraper::Selector> =
            lazy_selector!($selectors);
        &*__SELECTOR
    }};
}

macro_rules! lazy_regex {
    ($regex:literal $(,)?) => {
        ::once_cell::sync::Lazy::new(|| ::regex::Regex::new($regex).unwrap());
    };
}

macro_rules! static_regex {
    ($regex:literal $(,)?) => {{
        static REGEX: ::once_cell::sync::Lazy<::regex::Regex> = lazy_regex!($regex);
        &*REGEX
    }};
}

pub(crate) mod atcoder;
