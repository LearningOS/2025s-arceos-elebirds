#![no_std]

use core::ptr::NonNull;

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const PAGE_SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    count: usize,
}

impl<const PAGE_SIZE: usize> EarlyAllocator<PAGE_SIZE> {
    pub const fn new() -> Self {
        Self { 
            start: 0, 
            end: 0, 
            b_pos: 0, 
            p_pos: 0, 
            count: 0 
        }
    }
}

impl<const PAGE_SIZE: usize> BaseAllocator for EarlyAllocator<PAGE_SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = self.end;
        self.count = 0;
    }

    fn add_memory(&mut self, _start: usize, _size: usize) -> Result<(), AllocError> {
        unimplemented!()
    }
}

impl<const PAGE_SIZE: usize> ByteAllocator for EarlyAllocator<PAGE_SIZE> {
    fn alloc(&mut self, layout: core::alloc::Layout) -> allocator::AllocResult<core::ptr::NonNull<u8>> {
        let size = layout.size();
        if self.b_pos + size > self.p_pos { // 如果分配占用了页区，则没有足够的空间
            return Err(AllocError::NoMemory);
        }
        let ptr = self.b_pos as *mut u8; // 分配的内存地址
        self.b_pos += size; // 更新字节区位置
        self.count += 1; // 更新分配次数
        unsafe { Ok(NonNull::new_unchecked(ptr)) }
    }

    fn dealloc(&mut self, _ptr: NonNull<u8>, layout: core::alloc::Layout) {
        let size = layout.size();
        self.b_pos -= size; // 释放内存
        self.count -= 1; // 更新分配次数
        if self.count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const PAGE_SIZE: usize> PageAllocator for EarlyAllocator<PAGE_SIZE> {
    const PAGE_SIZE: usize = PAGE_SIZE;
    
    fn alloc_pages(&mut self, num_pages: usize, _align_pow2: usize) -> AllocResult<usize> {
        let size = num_pages * Self::PAGE_SIZE; // 申请的页数 * 页大小
        if self.p_pos - size < self.b_pos { // 如果分配占用了字节区，则没有足够的空间
            return Err(AllocError::NoMemory);
        }
        let ptr = self.p_pos - size; // 从后往前分配
        self.p_pos -= size;
        Ok(ptr)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        unimplemented!()
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / Self::PAGE_SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / Self::PAGE_SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / Self::PAGE_SIZE
    }
}
