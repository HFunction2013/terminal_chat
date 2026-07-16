fn int_print_content(content: &str) {
    println!("{}", content);
}
crate::define_hook_system! (
    int_print_content,
    "print_content",
    &mut String,
    &str,
    &str,
    &str
);