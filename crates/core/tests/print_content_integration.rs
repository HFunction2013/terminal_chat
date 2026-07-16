
use core::{
    clear_print_content_hooks, print_content, register_after_print_content, register_before_print_content, register_on_print_content,
};
use std::sync::{Arc, Mutex};

#[test]
fn before_chain_stops_but_on_and_after_run() {
    clear_print_content_hooks();

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);

    register_before_print_content("b1", move |s: &mut String| {
        l.lock().unwrap().push(s.clone());
        s.push_str("[B1]");
        false
    });

    register_before_print_content("b2", |_| true);

    register_on_print_content("o1", |_| false);

    print_content("hi");

    let out = log.lock().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], "hi");
}

#[test]
fn on_chain_stops_but_after_runs() {
    clear_print_content_hooks();

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);

    register_on_print_content("o1", |_| false);
    register_on_print_content("o2", |_| true);

    register_after_print_content("a1", move |s| {
        l.lock().unwrap().push(s.to_string());
        false
    });

    print_content("x");

    let out = log.lock().unwrap();
    assert_eq!(out.len(), 0); 
}

#[test]
fn after_chain_stops_silently() {
    clear_print_content_hooks();

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);

    register_after_print_content("a1", move |s| {
        l.lock().unwrap().push(format!("A1:{}", s));
        false
    });

    register_after_print_content("a2", |_| true);

    print_content("y");

    let out = log.lock().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], "A1:y");
}

#[test]
fn execution_order_is_registration_order() {
    clear_print_content_hooks();

    let seq: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let s1 = Arc::clone(&seq);
    register_before_print_content("b", move |_| {
        s1.lock().unwrap().push("before".into());
        false
    });

    let s2 = Arc::clone(&seq);
    register_on_print_content("o", move |_| {
        s2.lock().unwrap().push("on".into());
        false
    });

    let s3 = Arc::clone(&seq);
    register_after_print_content("a", move |_| {
        s3.lock().unwrap().push("after".into());
        false
    });

    print_content("z");

    let out = seq.lock().unwrap();
    assert_eq!(*out, vec!["before", "on", "after"]);
}

#[test]
fn clear_print_content_hooks_works() {
    clear_print_content_hooks();

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let l = Arc::clone(&log);

    register_before_print_content("b", move |_| {
        l.lock().unwrap().push("called".into());
        false
    });

    print_content("1");
    clear_print_content_hooks();
    print_content("2");

    let out = log.lock().unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0], "called");
}

#[test]
fn no_recursion_when_hook_calls_macro() {
    clear_print_content_hooks();

    let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let l_before = Arc::clone(&log);
    register_before_print_content("recursive", move |s: &mut String| {
        l_before.lock().unwrap().push("before".into());
        print_content("infinite");
        s.push('!');
        false
    });

    let l_after = Arc::clone(&log);
    register_after_print_content("capture", move |s| {
        l_after.lock().unwrap().push(s.to_string());
        false
    });

    print_content("x");

    let out = log.lock().unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0], "before");
    assert_eq!(out[1], "x!");
}

#[test]
fn multiple_invocations_are_independent() {
    clear_print_content_hooks();

    let counter: Arc<Mutex<i32>> = Arc::new(Mutex::new(0));
    let c = Arc::clone(&counter);

    register_before_print_content("inc", move |_| {
        *c.lock().unwrap() += 1;
        false
    });

    print_content("a");
    print_content("b");

    assert_eq!(*counter.lock().unwrap(), 2);
}