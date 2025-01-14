use crate::*;
use g::traits::ElementExt;
use gstreamer as g;
use gstreamer::glib::{translate::ToGlibPtr, Cast, ObjectType};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use tracing::{
    field::Visit,
    span::{Attributes, Record},
    Id, Subscriber,
};
use tracing::{Level, Metadata};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[derive(Default, Debug)]
struct KV {
    gobject_address: Option<usize>,
    gobject_type: Option<&'static str>,
    gstobject_name: Option<&'static str>,
    gstelement_state: Option<&'static str>,
    gstelement_pending_state: Option<&'static str>,
    gstpad_state: Option<&'static str>,
    gstpad_parent_name: Option<&'static str>,
    gstpad_parent_state: Option<&'static str>,
    gstpad_parent_pending_state: Option<&'static str>,
}

#[derive(Debug)]
struct GstEvent {
    message: &'static str,
    kvs: KV,
    level: Level,
    target: &'static str,
}

impl Visit for GstEvent {
    fn record_i64(&mut self, field: &tracing_core::Field, _: i64) {
        panic!("unexpected i64 field: {}", field.name());
    }

    fn record_u64(&mut self, field: &tracing_core::Field, value: u64) {
        if field.name() == "gobject.address" {
            assert_eq!(
                value,
                self.kvs
                    .gobject_address
                    .expect("gobject.address present but not expected") as u64
            );
        } else {
            panic!("unexpected u64 field: {}", field.name());
        }
    }

    fn record_bool(&mut self, field: &tracing_core::Field, _: bool) {
        panic!("unexpected boolean field: {}", field.name());
    }

    fn record_str(&mut self, field: &tracing_core::Field, value: &str) {
        let expected = match field.name() {
            "message" => self.message,
            "gobject.type" => self
                .kvs
                .gobject_type
                .expect("gobject.type present but not expected"),
            "gstobject.name" => self
                .kvs
                .gstobject_name
                .expect("gstobject.name present but not expected"),
            "gstelement.state" => self
                .kvs
                .gstelement_state
                .expect("gstelement.state present but not expected"),
            "gstelement.pending_state" => self
                .kvs
                .gstelement_pending_state
                .expect("gstelement.pending_state present but not expected"),
            "gstpad.parent.name" => self
                .kvs
                .gstpad_parent_name
                .expect("gstpad.parent.name present but not expected"),
            "gstpad.parent.state" => self
                .kvs
                .gstpad_parent_state
                .expect("gstpad.parent.state present but not expected"),
            "gstpad.parent.pending_state" => self
                .kvs
                .gstpad_parent_pending_state
                .expect("gstpad.parent.pending_state present but not expected"),
            _ => panic!("unexpected string field: {}", field.name()),
        };
        assert_eq!(value, expected);
    }

    fn record_error(&mut self, field: &tracing_core::Field, _: &(dyn std::error::Error + 'static)) {
        panic!("unexpected error field: {}", field.name());
    }

    fn record_debug(&mut self, field: &tracing_core::Field, d: &dyn std::fmt::Debug) {
        if field.name() == "gstpad.state" {
            let value = format!("{:?}", d);
            assert_eq!(
                value,
                self.kvs
                    .gstpad_state
                    .expect("gstpad.state present but not expected")
            )
        } else {
            panic!("unexpected debug field: {}", field.name());
        }
    }
}

#[derive(Debug)]
enum Expect {
    GstEvent(GstEvent),
}

struct MockSubscriber {
    expected: Arc<Mutex<VecDeque<Expect>>>,
    filter: fn(&Metadata<'_>) -> bool,
    name: &'static str,
}

impl MockSubscriber {
    fn new(filter: fn(&Metadata<'_>) -> bool, name: &'static str, expected: Vec<Expect>) -> Self {
        let expected = Arc::new(Mutex::new(expected.into()));
        MockSubscriber {
            expected,
            name,
            filter,
        }
    }

    fn with_expected<F>(
        filter: fn(&Metadata<'_>) -> bool,
        name: &'static str,
        cb: F,
        expected: Vec<Expect>,
    ) where
        F: FnOnce(),
    {
        let subscriber = Self::new(filter, name, expected);
        let expected = subscriber.expected.clone();
        let dispatch = tracing::Dispatch::new(subscriber);
        tracing::dispatcher::with_default(&dispatch, cb);
        let guard = expected.lock().expect("mutex lock");
        assert!(
            guard.is_empty(),
            "all of the expected messages should be present, these were not found: {:?}!",
            *guard
        );
    }
}

impl Subscriber for MockSubscriber {
    fn enabled(&self, meta: &tracing_core::Metadata<'_>) -> bool {
        meta.target().starts_with(crate::TARGET) && (self.filter)(meta)
    }
    fn event(&self, e: &tracing_core::Event<'_>) {
        println!("[{}] event: {:?}", self.name, e);
        match self.expected.lock().expect("mutex lock").pop_front() {
            None => {
                panic!("[{}] unexpected extra event received", self.name)
            }
            Some(Expect::GstEvent(mut expected @ GstEvent { .. })) => {
                let meta = e.metadata();
                if meta.target() != format!("gstreamer::{}", expected.target) {
                    panic!(
                        "[{}] event with target {} received, but expected {}",
                        self.name,
                        meta.target(),
                        expected.target,
                    );
                }
                if *meta.level() != expected.level {
                    panic!(
                        "[{}] event with level {} received, but expected {}",
                        self.name,
                        meta.level(),
                        expected.level,
                    );
                }
                e.record(&mut expected);
            }
        }
    }
    fn exit(&self, _: &Id) {
        todo!()
    }
    fn enter(&self, _: &Id) {
        todo!()
    }
    fn record_follows_from(&self, _: &Id, _: &Id) {
        todo!()
    }
    fn record(&self, _: &Id, _: &Record<'_>) {
        todo!()
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        todo!()
    }
}

impl<S: Subscriber> Layer<S> for MockSubscriber {
    fn enabled(&self, meta: &tracing_core::Metadata<'_>, _: Context<'_, S>) -> bool {
        Subscriber::enabled(self, meta)
    }

    fn on_event(&self, event: &tracing_core::Event<'_>, _: Context<'_, S>) {
        Subscriber::event(self, event)
    }
}

fn test_simple_error() {
    MockSubscriber::with_expected(
        |_| true,
        "test_simple_error",
        || {
            let cat = g::DebugCategory::new("test_error_cat", g::DebugColorFlags::empty(), None);
            g::error!(cat, "simple error");
        },
        vec![Expect::GstEvent(GstEvent {
            message: "simple error",
            kvs: Default::default(),
            level: Level::ERROR,
            target: "test_error_cat",
        })],
    );
}

fn test_simple_warning() {
    MockSubscriber::with_expected(
        |_| true,
        "test_simple_warning",
        || {
            let cat = g::DebugCategory::new("test_simple_cat", g::DebugColorFlags::empty(), None);
            g::warning!(cat, "simple warning");
        },
        vec![Expect::GstEvent(GstEvent {
            message: "simple warning",
            kvs: Default::default(),
            level: Level::WARN,
            target: "test_simple_cat",
        })],
    );
}

fn test_simple_events() {
    MockSubscriber::with_expected(
        |_| true,
        "test_simple_events",
        || {
            let cat = g::DebugCategory::new("test_simple_cat", g::DebugColorFlags::empty(), None);
            g::fixme!(cat, "simple fixme");
            g::info!(cat, "simple info");
            g::memdump!(cat, "simple memdump");
            g::trace!(cat, "simple trace");
        },
        vec![
            Expect::GstEvent(GstEvent {
                message: "simple fixme",
                kvs: Default::default(),
                level: Level::WARN,
                target: "test_simple_cat",
            }),
            Expect::GstEvent(GstEvent {
                message: "simple info",
                kvs: Default::default(),
                level: Level::INFO,
                target: "test_simple_cat",
            }),
            Expect::GstEvent(GstEvent {
                message: "simple memdump",
                kvs: Default::default(),
                level: Level::TRACE,
                target: "test_simple_cat",
            }),
            Expect::GstEvent(GstEvent {
                message: "simple trace",
                kvs: Default::default(),
                level: Level::TRACE,
                target: "test_simple_cat",
            }),
        ],
    );
}

fn test_with_object() {
    let p = g::Pipeline::new(None);
    let p_addr = p.as_object_ref().to_glib_none().0 as usize;
    MockSubscriber::with_expected(
        |m| m.target() == "gstreamer::test_object_cat",
        "test_with_object",
        move || {
            let cat = g::DebugCategory::new("test_object_cat", g::DebugColorFlags::empty(), None);
            g::error!(cat, obj: &p, "with object");
        },
        vec![Expect::GstEvent(GstEvent {
            message: "with object",
            kvs: KV {
                gobject_address: Some(p_addr),
                gobject_type: Some("GstPipeline"),
                gstobject_name: Some("pipeline0"),
                gstelement_state: Some("null"),
                gstelement_pending_state: Some("void-pending"),
                ..Default::default()
            },
            level: Level::ERROR,
            target: "test_object_cat",
        })],
    );
}

fn test_with_upcast_object() {
    let obj: gstreamer::glib::Object = g::Bin::new(None).upcast();
    let obj_addr = obj.as_object_ref().to_glib_none().0 as usize;
    MockSubscriber::with_expected(
        |m| m.target() == "gstreamer::test_object_cat",
        "test_with_upcast_object",
        move || {
            let cat = g::DebugCategory::new("test_object_cat", g::DebugColorFlags::empty(), None);
            g::error!(cat, obj: &obj, "with upcast object");
        },
        vec![Expect::GstEvent(GstEvent {
            message: "with upcast object",
            kvs: KV {
                gobject_address: Some(obj_addr),
                gobject_type: Some("GstBin"),
                gstobject_name: Some("bin0"),
                gstelement_state: Some("null"),
                gstelement_pending_state: Some("void-pending"),
                ..Default::default()
            },
            level: Level::ERROR,
            target: "test_object_cat",
        })],
    );
}

fn test_with_pad() {
    let pad = g::Pad::new(Some("custom_pad_name"), gstreamer::PadDirection::Sink);
    let parent = g::Bin::new(Some("custom_bin_name"));
    parent.add_pad(&pad).expect("add pad");
    let pad_addr = pad.as_object_ref().to_glib_none().0 as usize;
    MockSubscriber::with_expected(
        |m| m.target() == "gstreamer::test_pad_cat",
        "test_with_pad",
        move || {
            let cat = g::DebugCategory::new("test_pad_cat", g::DebugColorFlags::empty(), None);
            g::error!(cat, obj: &pad, "with pad object");
        },
        vec![Expect::GstEvent(GstEvent {
            message: "with pad object",
            kvs: KV {
                gobject_address: Some(pad_addr),
                gobject_type: Some("GstPad"),
                gstobject_name: Some("custom_pad_name"),
                gstpad_state: Some("{FLUSHING, NEED_PARENT}"),
                gstpad_parent_name: Some("custom_bin_name"),
                gstpad_parent_state: Some("null"),
                gstpad_parent_pending_state: Some("void-pending"),
                ..Default::default()
            },
            level: Level::ERROR,
            target: "test_pad_cat",
        })],
    );
}

fn test_disintegration() {
    MockSubscriber::with_expected(
        |m| m.target() == "gstreamer::disintegration",
        "test_disintegration",
        move || {
            let cat = g::DebugCategory::new("disintegration", g::DebugColorFlags::empty(), None);
            g::error!(cat, "apple");
            disintegrate_events();
            g::error!(cat, "banana");
            integrate_events();
            g::error!(cat, "chaenomeles");
        },
        vec![
            Expect::GstEvent(GstEvent {
                message: "apple",
                kvs: Default::default(),
                level: Level::ERROR,
                target: "disintegration",
            }),
            Expect::GstEvent(GstEvent {
                message: "chaenomeles",
                kvs: Default::default(),
                level: Level::ERROR,
                target: "disintegration",
            }),
        ],
    );
}

fn test_formatting() {
    MockSubscriber::with_expected(
        |_| true,
        "test_formatting",
        || {
            let cat = g::DebugCategory::new("ANSWERS", g::DebugColorFlags::empty(), None);
            g::warning!(cat, "the answer is believed to be {}.", 42);
        },
        vec![Expect::GstEvent(GstEvent {
            message: "the answer is believed to be 42.",
            kvs: Default::default(),
            level: Level::WARN,
            target: "ANSWERS",
        })],
    );
}

fn test_interests() {
    let mock_subscriber = MockSubscriber::new(
        |_| true,
        "test_interests",
        vec![
            Expect::GstEvent(GstEvent {
                message: "warnings should be visible",
                kvs: Default::default(),
                level: Level::WARN,
                target: "INTERESTS",
            }),
            Expect::GstEvent(GstEvent {
                message: "errors should be visible",
                kvs: Default::default(),
                level: Level::ERROR,
                target: "INTERESTS",
            }),
        ],
    );
    let expected = mock_subscriber.expected.clone();
    let subscriber = tracing_subscriber::registry().with(mock_subscriber).with(
        tracing_subscriber::filter::LevelFilter::from(tracing::Level::WARN),
    );
    let dispatch = tracing::Dispatch::new(subscriber);
    tracing::dispatcher::with_default(&dispatch, move || {
        let cat = g::DebugCategory::new("INTERESTS", g::DebugColorFlags::empty(), None);
        g::warning!(cat, "warnings should be visible");
        g::error!(cat, "errors should be visible");
        g::info!(cat, "infos should NOT be visible");
        g::debug!(cat, "debugs should NOT be visible");
        g::trace!(cat, "traces should NOT be visible");
    });
    let guard = expected.lock().expect("mutex lock");
    assert!(
        guard.is_empty(),
        "all of the expected messages should be present, these were not found: {:?}!",
        *guard
    );
}

// NB: we aren't using the test harness here to allow us for the necessary gstreamer setup more
// straightforwardly.
pub(crate) fn run() {
    g::debug_remove_default_log_function();
    g::init().expect("gst init");
    g::debug_set_default_threshold(g::DebugLevel::Memdump);
    integrate_events();
    test_simple_error();
    test_simple_warning();
    test_simple_events();
    test_with_object();
    test_with_upcast_object();
    test_with_pad();
    test_disintegration();
    test_formatting();
    test_interests();
    disintegrate_events();
}
