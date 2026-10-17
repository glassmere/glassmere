# English texts of `cargo run -p seaglass --example localization`.
title = Localization
intro = Texts come from Fluent catalogs in examples/locales. Plurals follow each language's rules.
language = Language
add = Add a file
remove = Remove a file
files = { $count ->
    [0] No files selected
    [one] { $count } file selected
   *[other] { $count } files selected
}
weight = Weight
saved = Saved { $name }
save = Save
name = Name
