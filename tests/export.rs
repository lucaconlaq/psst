#[test]
fn dotenv_values_round_trip_without_interpolation() {
    for value in [
        "plain",
        "a\nb\rc",
        "quotes \" ' # $HOME ${HOME}",
        "\\n\\$HOME",
        "tabs\tunicode 🔑",
        "",
        " spaced ",
    ] {
        let line = format!("KEY={}\n", psst::operations::dotenv(value));
        let parsed = dotenvy::from_read_iter(line.as_bytes())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(parsed.0, "KEY");
        assert_eq!(parsed.1, value);
    }
}
