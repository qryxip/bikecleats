macro_rules! lazy_url {
    ($url:literal) => {
        ::once_cell::sync::Lazy::new(|| ::std::primitive::str::parse::<::url::Url>($url).unwrap())
    };
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

macro_rules! static_selector {
    ($selectors:literal $(,)?) => {{
        static __SELECTOR: ::once_cell::sync::Lazy<::scraper::Selector> =
            ::once_cell::sync::Lazy::new(|| ::scraper::Selector::parse($selectors).unwrap());
        &*__SELECTOR
    }};
}

pub(crate) mod atcoder;
