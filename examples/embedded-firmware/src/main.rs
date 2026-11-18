//! The firmware: a heap, a panic handler and the board. The board here does
//! nothing; put your HAL's display, touch controller and timer behind it.
#![cfg_attr(target_os = "none", no_std, no_main)]

/// The workspace's own tools build every target for the host as well.
#[cfg(not(target_os = "none"))]
fn main() {
    eprintln!(
        "embedded-firmware is built for a bare-metal target; try `cargo run --example embedded`"
    );
}

#[cfg(target_os = "none")]
mod firmware {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;
    use core::sync::atomic::{AtomicUsize, Ordering};
    use core::time::Duration;
    use embedded_firmware::{Board, run};
    use glassmere_embedded::Rgb565;

    const HEAP: usize = 96 * 1024;

    /// The simplest heap that works for a check: it never frees. Real firmware
    /// uses an allocator such as `embedded-alloc`.
    struct Bump {
        memory: UnsafeCell<[u8; HEAP]>,
        used: AtomicUsize,
    }

    // SAFETY: the firmware has one thread, and `used` hands out each byte once.
    unsafe impl Sync for Bump {}

    // SAFETY: every block returned is inside `memory`, aligned as asked, and
    // overlaps no other, because `used` only grows.
    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let base = self.memory.get() as usize;
            let mut used = self.used.load(Ordering::Relaxed);
            loop {
                let start = (base + used).next_multiple_of(layout.align()) - base;
                let end = start + layout.size();
                if end > HEAP {
                    return core::ptr::null_mut();
                }
                match self
                    .used
                    .compare_exchange(used, end, Ordering::Relaxed, Ordering::Relaxed)
                {
                    Ok(_) => return (base + start) as *mut u8,
                    Err(now) => used = now,
                }
            }
        }

        unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
    }

    #[global_allocator]
    static ALLOCATOR: Bump = Bump {
        memory: UnsafeCell::new([0; HEAP]),
        used: AtomicUsize::new(0),
    };

    #[panic_handler]
    fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
        loop {}
    }

    struct NoBoard;

    impl Board for NoBoard {
        fn now(&mut self) -> Duration {
            Duration::ZERO
        }
        fn touch(&mut self) -> Option<(u16, u16)> {
            None
        }
        fn temperature(&mut self) -> Option<i32> {
            None
        }
        fn flush(&mut self, _x: u16, _y: u16, _width: u16, _pixels: &[Rgb565]) {}
        fn sleep(&mut self, _until: Option<Duration>) {}
    }

    /// Called by the runtime of the chip (`cortex-m-rt` and the like) once
    /// memory is set up.
    #[unsafe(no_mangle)]
    pub extern "C" fn main() -> ! {
        run(NoBoard)
    }
}
