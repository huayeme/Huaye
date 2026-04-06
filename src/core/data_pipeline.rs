use std::collections::VecDeque;

/// 高性能的泛型环形缓冲区，用于防止 OOM
/// 支持 O(1) 复杂度的头尾写入和淘汰
#[allow(dead_code)]
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    capacity: usize,
}

#[allow(dead_code)]
impl<T> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "RingBuffer capacity must be > 0");
        Self {
            buffer: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// 压入新数据，如果满则弹出最旧的数据
    pub fn push(&mut self, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    /// 批量压入数据，自动跳过超出容量的头部元素
    pub fn push_batch(&mut self, items: impl IntoIterator<Item = T>) {
        let items: Vec<T> = items.into_iter().collect();
        let skip = items.len().saturating_sub(self.capacity);
        for item in items.into_iter().skip(skip) {
            self.push(item);
        }
    }

    /// 获取当前存储的数据量
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// 获取底层数据的只读切片 (可能是不连续的两段)
    pub fn as_slices(&self) -> (&[T], &[T]) {
        self.buffer.as_slices()
    }

    /// 清空缓冲区
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity() {
        let mut rb = RingBuffer::new(3);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.len(), 3);

        rb.push(4);
        assert_eq!(rb.len(), 3);

        let (s1, s2) = rb.as_slices();
        let mut items = Vec::new();
        items.extend_from_slice(s1);
        items.extend_from_slice(s2);
        assert_eq!(items, vec![2, 3, 4]);
    }

    #[test]
    fn test_push_batch() {
        let mut rb = RingBuffer::new(5);
        rb.push_batch(vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(rb.len(), 5);

        let (s1, s2) = rb.as_slices();
        let mut items = Vec::new();
        items.extend_from_slice(s1);
        items.extend_from_slice(s2);
        assert_eq!(items, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_ring_buffer_capacity_one() {
        let mut rb = RingBuffer::new(1);
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);

        rb.push(42);
        assert_eq!(rb.len(), 1);
        assert!(!rb.is_empty());

        rb.push(99);
        assert_eq!(rb.len(), 1);

        let (s1, s2) = rb.as_slices();
        let mut items = Vec::new();
        items.extend_from_slice(s1);
        items.extend_from_slice(s2);
        assert_eq!(items, vec![99]);
    }

    #[test]
    fn test_ring_buffer_clear() {
        let mut rb = RingBuffer::new(10);
        rb.push(1);
        rb.push(2);
        rb.push(3);
        assert_eq!(rb.len(), 3);

        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.len(), 0);
    }

    #[test]
    fn test_ring_buffer_empty_pop() {
        let mut rb: RingBuffer<i32> = RingBuffer::new(5);
        assert!(rb.is_empty());

        rb.push(1);
        assert!(!rb.is_empty());

        let (s1, s2) = rb.as_slices();
        let mut items = Vec::new();
        items.extend_from_slice(s1);
        items.extend_from_slice(s2);
        assert_eq!(items, vec![1]);
    }

    #[test]
    fn test_ring_buffer_fuzzy() {
        let mut rb = RingBuffer::new(100);
        for i in 0..1000 {
            rb.push(i);
        }
        assert_eq!(rb.len(), 100);

        let (s1, s2) = rb.as_slices();
        let mut items = Vec::new();
        items.extend_from_slice(s1);
        items.extend_from_slice(s2);
        assert_eq!(items.len(), 100);
        assert_eq!(items[0], 900);
        assert_eq!(items[99], 999);
    }

    #[test]
    #[should_panic(expected = "capacity must be > 0")]
    fn test_ring_buffer_zero_capacity() {
        let _rb = RingBuffer::<i32>::new(0);
    }
}
