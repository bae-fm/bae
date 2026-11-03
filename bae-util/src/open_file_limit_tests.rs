use super::*;

fn current_limit() -> libc::rlimit {
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit writes the limit into the struct we pass.
    assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) }, 0);
    limit
}

/// Starting below its ceiling, the soft limit is raised to exactly the
/// ceiling. The test process's limit is lowered to a value every other
/// test in it still fits under, then raised again.
#[test]
fn a_soft_limit_below_its_ceiling_is_raised_to_it() {
    let hard = current_limit().rlim_max;
    let ceiling = soft_ceiling(hard).expect("soft ceiling");
    let lowered = libc::rlimit { rlim_cur: ceiling.min(4096) - 1, rlim_max: hard };
    // SAFETY: setrlimit reads the struct we pass.
    assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) }, 0);

    raise_open_file_limit();

    assert_eq!(current_limit().rlim_cur, ceiling);
}
