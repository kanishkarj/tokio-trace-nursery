extern crate futures;
#[cfg(feature = "with-tokio")]
extern crate tokio;
#[cfg_attr(test, macro_use)]
extern crate tokio_trace;

use futures::{Async, Future, Poll, Sink, StartSend, Stream};
use tokio_trace::{dispatcher, Dispatch, Span};

pub mod executor;

// TODO: seal?
pub trait Instrument: Sized {
    fn instrument(self, span: Span) -> Instrumented<Self> {
        Instrumented { inner: self, span }
    }
}

pub trait WithSubscriber: Sized {
    fn with_subscriber<S>(self, subscriber: S) -> WithDispatch<Self>
    where
        S: Into<Dispatch>,
    {
        WithDispatch {
            inner: self,
            dispatch: subscriber.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Instrumented<T> {
    inner: T,
    span: Span,
}

#[derive(Clone, Debug)]
pub struct WithDispatch<T> {
    inner: T,
    dispatch: Dispatch,
}

impl<T: Sized> Instrument for T {}

impl<T: Future> Future for Instrumented<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let span = &mut self.span;
        let inner = &mut self.inner;
        span.enter(|| inner.poll())
    }
}

impl<T: Stream> Stream for Instrumented<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        let span = &mut self.span;
        let inner = &mut self.inner;
        span.enter(|| inner.poll())
    }
}

impl<T: Sink> Sink for Instrumented<T> {
    type SinkItem = T::SinkItem;
    type SinkError = T::SinkError;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        let span = &mut self.span;
        let inner = &mut self.inner;
        span.enter(|| inner.start_send(item))
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        let span = &mut self.span;
        let inner = &mut self.inner;
        span.enter(|| inner.poll_complete())
    }
}

impl<T> Instrumented<T> {
    /// Borrows the `Span` that this type is instrumented by.
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// Mutably borrows the `Span` that this type is instrumented by.
    pub fn span_mut(&mut self) -> &mut Span {
        &mut self.span
    }

    /// Consumes the `Instrumented`, returning the wrapped type.
    ///
    /// Note that this drops the span.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Sized> WithSubscriber for T {}

impl<T: Future> Future for WithDispatch<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let inner = &mut self.inner;
        dispatcher::with_default(&self.dispatch, || inner.poll())
    }
}

impl<T> WithDispatch<T> {
    pub(crate) fn with_dispatch<U: Sized>(&self, inner: U) -> WithDispatch<U> {
        WithDispatch {
            dispatch: self.dispatch.clone(),
            inner,
        }
    }

    /// Borrows the `Dispatch` that this type is instrumented by.
    pub fn dispatch(&self) -> &Dispatch {
        &self.dispatch
    }

    /// Consumes the `WithDispatch`, returning the wrapped type.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests {
    extern crate tokio;

    use super::{test_support::*, *};
    use futures::{future, stream, task};
    use tokio_trace::subscriber::with_default;

    struct PollN<T, E> {
        and_return: Option<Result<T, E>>,
        finish_at: usize,
        polls: usize,
    }

    impl<T, E> Future for PollN<T, E> {
        type Item = T;
        type Error = E;
        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            self.polls += 1;
            if self.polls == self.finish_at {
                self.and_return
                    .take()
                    .expect("polled after ready")
                    .map(Async::Ready)
            } else {
                task::current().notify();
                Ok(Async::NotReady)
            }
        }
    }

    impl PollN<(), ()> {
        fn new_ok(finish_at: usize) -> Self {
            Self {
                and_return: Some(Ok(())),
                finish_at,
                polls: 0,
            }
        }

        fn new_err(finish_at: usize) -> Self {
            Self {
                and_return: Some(Err(())),
                finish_at,
                polls: 0,
            }
        }
    }

    #[test]
    fn future_enter_exit_is_reasonable() {
        let (subscriber, handle) = subscriber::mock()
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .drop_span(span::mock().named("foo"))
            .done()
            .run_with_handle();
        with_default(subscriber, || {
            PollN::new_ok(2).instrument(span!("foo")).wait().unwrap();
        });
        handle.assert_finished();
    }

    #[test]
    fn future_error_ends_span() {
        let (subscriber, handle) = subscriber::mock()
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .drop_span(span::mock().named("foo"))
            .done()
            .run_with_handle();
        with_default(subscriber, || {
            PollN::new_err(2)
                .instrument(span!("foo"))
                .wait()
                .unwrap_err();
        });

        handle.assert_finished();
    }

    #[test]
    fn stream_enter_exit_is_reasonable() {
        let (subscriber, handle) = subscriber::mock()
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .enter(span::mock().named("foo"))
            .exit(span::mock().named("foo"))
            .drop_span(span::mock().named("foo"))
            .run_with_handle();
        with_default(subscriber, || {
            stream::iter_ok::<_, ()>(&[1, 2, 3])
                .instrument(span!("foo"))
                .for_each(|_| future::ok(()))
                .wait()
                .unwrap();
        });
        handle.assert_finished();
    }

    #[test]
    fn span_follows_future_onto_threadpool() {
        let (subscriber, handle) = subscriber::mock()
            .enter(span::mock().named("a"))
            .enter(span::mock().named("b"))
            .exit(span::mock().named("b"))
            .enter(span::mock().named("b"))
            .exit(span::mock().named("b"))
            .drop_span(span::mock().named("b"))
            .exit(span::mock().named("a"))
            .drop_span(span::mock().named("a"))
            .done()
            .run_with_handle();
        let mut runtime = tokio::runtime::Runtime::new().unwrap();
        with_default(subscriber, || {
            span!("a").enter(|| {
                let future = PollN::new_ok(2).instrument(span!("b")).map(|_| {
                    span!("c").enter(|| {
                        // "c" happens _outside_ of the instrumented future's
                        // spab, so we don't expect it.
                    })
                });
                runtime.block_on(Box::new(future)).unwrap();
            })
        });
        handle.assert_finished();
    }
}
