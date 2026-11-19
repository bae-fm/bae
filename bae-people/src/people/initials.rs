/// The initials an avatar draws while a person has no picture: the first letter
/// of up to two words of `name`, in upper case.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .flat_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn one_word_gives_one_letter() {
        assert_eq!(initials("Ana"), "A");
        assert_eq!(initials("ana"), "A");
    }

    #[test]
    fn two_words_give_two_letters_and_more_words_add_none() {
        assert_eq!(initials("Ana Maria"), "AM");
        assert_eq!(initials("ana maria lopez"), "AM");
    }

    #[test]
    fn any_run_of_whitespace_separates_words() {
        assert_eq!(initials("  Ben \t Cole  "), "BC");
    }

    #[test]
    fn an_address_gives_its_first_letter() {
        assert_eq!(initials("another@example.com"), "A");
    }

    #[test]
    fn a_blank_name_gives_none() {
        assert_eq!(initials(""), "");
        assert_eq!(initials("   "), "");
    }

    #[test]
    fn a_letter_that_upper_cases_to_two_gives_both() {
        assert_eq!(initials("élodie"), "É");
        assert_eq!(initials("ß"), "SS");
    }
}
