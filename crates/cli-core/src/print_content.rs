fn internal_print_content(content: &str) {
    println!("{}", content);
}
crate::define_hook_system!(
    internal_print_content,
    "print_content",
    M,
    R,
    R,
    R,
    &mut String,
    &String,
    &String,
    String,
    ()
);
