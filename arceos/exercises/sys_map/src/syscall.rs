#![allow(dead_code)]

use core::ffi::{c_void, c_char, c_int};
use axhal::arch::TrapFrame;
use axhal::trap::{register_trap_handler, SYSCALL};
use axerrno::LinuxError;
use axtask::current;
use axtask::TaskExtRef;
use axhal::paging::MappingFlags;
use arceos_posix_api::{self as api, get_file_like};
use memory_addr::{MemoryAddr, VirtAddr, VirtAddrRange};
use axstd::vec;

const SYS_IOCTL: usize = 29;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_READ: usize = 63;
const SYS_WRITE: usize = 64;
const SYS_WRITEV: usize = 66;
const SYS_EXIT: usize = 93;
const SYS_EXIT_GROUP: usize = 94;
const SYS_SET_TID_ADDRESS: usize = 96;
const SYS_MMAP: usize = 222;

const AT_FDCWD: i32 = -100;

/// Macro to generate syscall body
///
/// It will receive a function which return Result<_, LinuxError> and convert it to
/// the type which is specified by the caller.
#[macro_export]
macro_rules! syscall_body {
    ($fn: ident, $($stmt: tt)*) => {{
        #[allow(clippy::redundant_closure_call)]
        let res = (|| -> axerrno::LinuxResult<_> { $($stmt)* })();
        match res {
            Ok(_) | Err(axerrno::LinuxError::EAGAIN) => debug!(concat!(stringify!($fn), " => {:?}"),  res),
            Err(_) => info!(concat!(stringify!($fn), " => {:?}"), res),
        }
        match res {
            Ok(v) => v as _,
            Err(e) => {
                -e.code() as _
            }
        }
    }};
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// permissions for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapProt: i32 {
        /// Page can be read.
        const PROT_READ = 1 << 0;
        /// Page can be written.
        const PROT_WRITE = 1 << 1;
        /// Page can be executed.
        const PROT_EXEC = 1 << 2;
    }
}

impl From<MmapProt> for MappingFlags {
    fn from(value: MmapProt) -> Self {
        let mut flags = MappingFlags::USER;
        if value.contains(MmapProt::PROT_READ) {
            flags |= MappingFlags::READ;
        }
        if value.contains(MmapProt::PROT_WRITE) {
            flags |= MappingFlags::WRITE;
        }
        if value.contains(MmapProt::PROT_EXEC) {
            flags |= MappingFlags::EXECUTE;
        }
        flags
    }
}

bitflags::bitflags! {
    #[derive(Debug)]
    /// flags for sys_mmap
    ///
    /// See <https://github.com/bminor/glibc/blob/master/bits/mman.h>
    struct MmapFlags: i32 {
        /// Share changes
        const MAP_SHARED = 1 << 0;
        /// Changes private; copy pages on write.
        const MAP_PRIVATE = 1 << 1;
        /// Map address must be exactly as requested, no matter whether it is available.
        const MAP_FIXED = 1 << 4;
        /// Don't use a file.
        const MAP_ANONYMOUS = 1 << 5;
        /// Don't check for reservations.
        const MAP_NORESERVE = 1 << 14;
        /// Allocation is for a stack.
        const MAP_STACK = 0x20000;
    }
}

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &TrapFrame, syscall_num: usize) -> isize {
    ax_println!("handle_syscall [{}] ...", syscall_num);
    let ret = match syscall_num {
         SYS_IOCTL => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _) as _,
        SYS_SET_TID_ADDRESS => sys_set_tid_address(tf.arg0() as _),
        SYS_OPENAT => sys_openat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _, tf.arg3() as _),
        SYS_CLOSE => sys_close(tf.arg0() as _),
        SYS_READ => sys_read(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITE => sys_write(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_WRITEV => sys_writev(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        SYS_EXIT_GROUP => {
            ax_println!("[SYS_EXIT_GROUP]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_EXIT => {
            ax_println!("[SYS_EXIT]: system is exiting ..");
            axtask::exit(tf.arg0() as _)
        },
        SYS_MMAP => sys_mmap(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        _ => {
            ax_println!("Unimplemented syscall: {}", syscall_num);
            -LinuxError::ENOSYS.code() as _
        }
    };
    ret
}

/// mmap: 将文件或设备映射到进程的地址空间
/// 
/// 参数:
/// - addr: 映射的地址
/// - length: 映射的长度
/// - prot: 保护属性
/// - flags: 映射标志
/// - fd: 文件描述符
#[allow(unused_variables)]
fn sys_mmap(
    addr: *mut usize,
    length: usize,
    prot: i32,
    _flags: i32,
    fd: i32,
    _offset: isize,
) -> isize {
    let curr = current();
    let mut uspace = curr.task_ext().aspace.lock();
    
    // 在当前进程的虚拟地址空间中，寻找一段空闲的满足要求的连续的虚拟地址
    // 如果 addr 为 NULL，则内核选择（页面对齐）创建映射的地址;这是最便携的创建新映射的方法。
    // 如果 addr 不是 NULL，则 kernel 将其视为放置 Map 位置的提示;
    // 在Linux，内核会选择附近的页面边界（但总是 大于或等于 /proc/sys/vm/mmap_min_addr）并尝试创建映射。
    // 如果那里已经存在另一个映射，则内核会选择 一个可能取决于也可能不取决于 hint 的新地址。
    let start = VirtAddr::from_usize(addr as usize + 0x10_0000).align_down_4k();
    let size = length.align_up_4k();
    let Some(vaddr) = uspace.find_free_area(start, size, 
        VirtAddrRange::from_start_size(uspace.base(), uspace.size())
    ) else {
        ax_println!("mmap: no free area");
        return -LinuxError::ENOMEM.code() as _;
    };

    ax_println!("expected addr: 0x{:x}, size: {}", addr as usize, length);
    ax_println!("got addr: 0x{:x}, size: {}", vaddr.as_usize(), size);

    // 分配内存空间
    let prot = MmapProt::from_bits_truncate(prot);
    if let Err(e) = uspace.map_alloc(vaddr, size, prot.into(), true) {
        ax_println!("mmap: map memory failed: {}", e);
        return -LinuxError::ENOMEM.code() as _;
    };

    // 读取文件内容到缓冲区
    let mut buf = vec![0; length];
    let Ok(file) = get_file_like(fd) else {
        ax_println!("mmap: invalid file descriptor");
        return -LinuxError::EBADF.code() as _;
    };
    if let Err(e) = file.read(&mut buf) {
        ax_println!("mmap: read file failed: {}", e);
        return -LinuxError::EIO.code() as _;
    };

    // 将缓冲区内容写入到内存区域
    if let Err(e) = uspace.write(vaddr, &buf) {
        ax_println!("mmap: write memory failed: {}", e);
        return -LinuxError::EIO.code() as _;
    };

    ax_println!("mmap: write memory success");

    vaddr.as_usize() as isize
}

fn sys_openat(dfd: c_int, fname: *const c_char, flags: c_int, mode: api::ctypes::mode_t) -> isize {
    assert_eq!(dfd, AT_FDCWD);
    api::sys_open(fname, flags, mode) as isize
}

fn sys_close(fd: i32) -> isize {
    api::sys_close(fd) as isize
}

fn sys_read(fd: i32, buf: *mut c_void, count: usize) -> isize {
    api::sys_read(fd, buf, count)
}

fn sys_write(fd: i32, buf: *const c_void, count: usize) -> isize {
    api::sys_write(fd, buf, count)
}

fn sys_writev(fd: i32, iov: *const api::ctypes::iovec, iocnt: i32) -> isize {
    unsafe { api::sys_writev(fd, iov, iocnt) }
}

fn sys_set_tid_address(tid_ptd: *const i32) -> isize {
    let curr = current();
    curr.task_ext().set_clear_child_tid(tid_ptd as _);
    curr.id().as_u64() as isize
}

fn sys_ioctl(_fd: i32, _op: usize, _argp: *mut c_void) -> i32 {
    ax_println!("Ignore SYS_IOCTL");
    0
}
