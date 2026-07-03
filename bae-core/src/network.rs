/// Upgrade HTTP URLs to HTTPS for App Transport Security compliance
pub fn upgrade_to_https(url: &str) -> String {
    url.strip_prefix("http://")
        .map_or_else(|| url.to_string(), |rest| format!("https://{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_to_https_rewrites_only_the_scheme_prefix() {
        assert_eq!(
            upgrade_to_https("http://host/image.jpg"),
            "https://host/image.jpg"
        );
        assert_eq!(
            upgrade_to_https("https://host/image.jpg"),
            "https://host/image.jpg"
        );
        assert_eq!(
            upgrade_to_https("http://host/image.jpg?src=http://other/image.jpg"),
            "https://host/image.jpg?src=http://other/image.jpg"
        );
    }
}
