use windows::Win32::System::Threading::GetCurrentThreadId;

pub fn current_thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_non_zero_thread_id() {
        let id = current_thread_id();
        assert!(id > 0, "thread id must be positive");
    }
}
