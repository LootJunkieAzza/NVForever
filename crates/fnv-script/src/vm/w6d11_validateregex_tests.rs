use super::regex_error_code;

#[test]
fn validateregex_maps_observed_syntax_errors_including_bad_brace_code_8() {
    let err_of = |pat: &str| {
        let e = fancy_regex::Regex::new(pat).unwrap_err();
        format!("{e} {e:?}")
    };
    assert_eq!(regex_error_code(&err_of("[abc")), 5.0);
    assert_eq!(regex_error_code(&err_of("(abc")), 6.0);
    assert_eq!(regex_error_code(&err_of("a{3,1}")), 8.0);
    assert_eq!(regex_error_code(&err_of("[z-a]")), 9.0);
    assert_eq!(regex_error_code(&err_of("*abc")), 11.0);
    assert_eq!(regex_error_code(&err_of(r"\x")), 3.0);
}
