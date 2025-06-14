use std::collections::HashMap;

/// Maps currencies to their primary country codes
pub fn get_currency_country_mapping() -> HashMap<&'static str, &'static str> {
    let mut mapping = HashMap::new();

    // Primary currency to country mappings (ISO 4217 to ISO 3166-1 alpha-2)
    mapping.insert("USD", "US"); // United States Dollar
    mapping.insert("EUR", "EU"); // Euro (using EU as representative)
    mapping.insert("GBP", "GB"); // British Pound Sterling
    mapping.insert("INR", "IN"); // Indian Rupee
    mapping.insert("SGD", "SG"); // Singapore Dollar
    mapping.insert("AED", "AE"); // UAE Dirham
    mapping.insert("JPY", "JP"); // Japanese Yen
    mapping.insert("AUD", "AU"); // Australian Dollar
    mapping.insert("CAD", "CA"); // Canadian Dollar
    mapping.insert("CHF", "CH"); // Swiss Franc

    mapping
}

/// Get the country code for a given currency
pub fn get_country_for_currency(currency: &str) -> Option<&'static str> {
    let mapping = get_currency_country_mapping();
    mapping.get(currency).copied()
}

/// Determine if two currencies represent different countries/regions
pub fn is_cross_border_by_currency(from_currency: &str, to_currency: &str) -> bool {
    if from_currency == to_currency {
        return false;
    }

    let from_country = get_country_for_currency(from_currency);
    let to_country = get_country_for_currency(to_currency);

    match (from_country, to_country) {
        (Some(from), Some(to)) => from != to,
        _ => true, // If we can't determine the country, assume cross-border for safety
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currency_mapping() {
        assert_eq!(get_country_for_currency("USD"), Some("US"));
        assert_eq!(get_country_for_currency("INR"), Some("IN"));
        assert_eq!(get_country_for_currency("EUR"), Some("EU"));
        assert_eq!(get_country_for_currency("INVALID"), None);
    }

    #[test]
    fn test_cross_border_detection() {
        // Same currency - not cross border
        assert_eq!(is_cross_border_by_currency("USD", "USD"), false);

        // Different currencies - cross border
        assert_eq!(is_cross_border_by_currency("USD", "INR"), true);
        assert_eq!(is_cross_border_by_currency("EUR", "GBP"), true);

        // Unknown currency - assume cross border
        assert_eq!(is_cross_border_by_currency("USD", "UNKNOWN"), true);
        assert_eq!(is_cross_border_by_currency("UNKNOWN", "INR"), true);
    }
}
