use gstreamer::{glib, subclass::prelude::*};

use crate::tracer::{TracingTracer, TracingTracerImpl};

#[derive(Default)]
pub struct FmtTracer { }

#[glib::object_subclass]
impl ObjectSubclass for FmtTracer {
    const NAME: &'static str = "FmtTracer";
    type Type = super::FmtTracer;
    type ParentType = TracingTracer;
    type Interfaces = ();
}

impl ObjectImpl for FmtTracer {
    fn constructed(&self) {
        tracing_subscriber::fmt::init();

        self.parent_constructed();
    }
}

impl GstObjectImpl for FmtTracer {}
impl TracerImpl for FmtTracer {}
impl TracingTracerImpl for FmtTracer {}
