use std::{
    cell::UnsafeCell,
    collections::VecDeque,
    io,
    marker::PhantomData,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    task::{Context, Poll, Waker},
};

type Inner<T> = Rc<(
    UnsafeCell<VecDeque<T>>,
    UnsafeCell<Option<Waker>>,
    UnsafeCell<usize>,
)>;

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let rx = Receiver::new();
    (rx.get_sender(), rx)
}

pub struct Receiver<T> {
    inner: Inner<T>,
    queue: NonNull<VecDeque<T>>,
    waker: NonNull<Option<Waker>>,
    cnt: NonNull<usize>,
}

impl<T> Receiver<T> {
    pub fn new() -> Self {
        let inner = Rc::new((
            UnsafeCell::new(VecDeque::new()),
            UnsafeCell::new(None),
            UnsafeCell::new(0),
        ));
        Self {
            queue: NonNull::new(inner.0.get()).unwrap(),
            waker: NonNull::new(inner.1.get()).unwrap(),
            cnt: NonNull::new(inner.2.get()).unwrap(),
            inner,
        }
    }

    pub fn get_sender(&self) -> Sender<T> {
        unsafe {
            *self.cnt.as_ptr() += 1;
        }
        Sender {
            inner: self.inner.clone(),
            queue: self.queue,
            waker: self.waker,
            cnt: self.cnt,
        }
    }

    pub fn recv<'a>(&'a self) -> RecvFuture<'a, T> {
        RecvFuture {
            queue: self.queue.as_ptr(),
            waker: self.waker.as_ptr(),
            cnt: self.cnt.as_ptr(),
            phantom: PhantomData,
        }
    }
}

pub struct RecvFuture<'a, T> {
    queue: *mut VecDeque<T>,
    waker: *mut Option<Waker>,
    cnt: *mut usize,
    phantom: PhantomData<&'a ()>,
}

impl<'a, T> Future for RecvFuture<'a, T> {
    type Output = io::Result<T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let Some(v) = (*self.queue).pop_front() else {
                if *self.cnt == 0 {
                    return Err(io::ErrorKind::BrokenPipe.into()).into();
                } else {
                    match &*self.waker {
                        Some(old) if old.will_wake(cx.waker()) => (),
                        _ => *self.waker = Some(cx.waker().clone()),
                    }
                    return Poll::Pending;
                }
            };
            Poll::Ready(Ok(v))
        }
    }
}

impl<T> Default for Receiver<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        unsafe {
            *self.cnt.as_ptr() = 0;
        }
    }
}

pub struct Sender<T> {
    inner: Inner<T>,
    queue: NonNull<VecDeque<T>>,
    waker: NonNull<Option<Waker>>,
    cnt: NonNull<usize>,
}

impl<T> Sender<T> {
    pub fn send(&self, v: T) -> io::Result<()> {
        unsafe {
            if *self.cnt.as_ptr() == 0 {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            (*self.queue.as_ptr()).push_back(v);
            if let Some(w) = (*self.waker.as_ptr()).take() {
                w.wake()
            }
        }
        Ok(())
    }

    pub fn is_closed(&self) -> bool {
        unsafe { *self.cnt.as_ptr() == 0 }
    }

    pub fn close(&self) {
        unsafe {
            *self.cnt.as_ptr() = 0;
            if let Some(w) = (*self.waker.as_ptr()).take() {
                w.wake()
            }
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        unsafe {
            if *self.cnt.as_ptr() <= 1 {
                *self.cnt.as_ptr() = 0;
                if let Some(w) = (*self.waker.as_ptr()).take() {
                    w.wake()
                }
            } else {
                *self.cnt.as_ptr() -= 1;
            }
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        unsafe {
            *self.cnt.as_ptr() += 1;
        }
        Self {
            inner: self.inner.clone(),
            queue: self.queue,
            waker: self.waker,
            cnt: self.cnt,
        }
    }
}
