use gst::prelude::*;
use gtk::{
    glib::{self, clone, Continue, GEnum},
    subclass::prelude::*,
};

use crate::backend::{KhaScreencastPortal, KhaSettings, Stream};

#[derive(Debug, PartialEq, Clone, Copy, GEnum)]
#[genum(type_name = "RecorderState")]
pub enum RecorderState {
    Null,
    Paused,
    Playing,
}

impl Default for RecorderState {
    fn default() -> Self {
        RecorderState::Null
    }
}

mod imp {
    use super::*;

    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    use crate::widgets::KhaAreaSelector;

    #[derive(Debug)]
    pub struct KhaRecorder {
        pub settings: KhaSettings,
        pub area_selector: KhaAreaSelector,
        pub portal: KhaScreencastPortal,
        pub pipeline: Rc<RefCell<Option<gst::Pipeline>>>,
        pub state: RefCell<RecorderState>,
        pub is_readying: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KhaRecorder {
        const NAME: &'static str = "KhaRecorder";
        type Type = super::KhaRecorder;
        type ParentType = glib::Object;

        fn new() -> Self {
            Self {
                settings: KhaSettings::new(),
                area_selector: KhaAreaSelector::new(),
                portal: KhaScreencastPortal::new(),
                pipeline: Rc::new(RefCell::new(None)),
                state: RefCell::new(RecorderState::default()),
                is_readying: Cell::new(false),
            }
        }
    }

    impl ObjectImpl for KhaRecorder {
        fn constructed(&self, obj: &Self::Type) {
            self.portal
                .connect_local(
                    "ready",
                    false,
                    clone!(@weak obj => @default-return None, move | args | {
                        let stream = args[1].get().unwrap();
                        obj.build_pipeline(stream);
                        None
                    }),
                )
                .unwrap();
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("ready", &[], <()>::static_type().into()).build(),
                    Signal::builder(
                        "record-success",
                        &[String::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                    Signal::builder(
                        "record-failed",
                        &[String::static_type().into()],
                        <()>::static_type().into(),
                    )
                    .build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpec::new_object(
                        "pipeline",
                        "pipeline",
                        "Pipeline",
                        gst::Pipeline::static_type(),
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_enum(
                        "state",
                        "state",
                        "State",
                        RecorderState::static_type(),
                        RecorderState::default() as i32,
                        glib::ParamFlags::READWRITE,
                    ),
                    glib::ParamSpec::new_boolean(
                        "is-readying",
                        "is-readying",
                        "Is readying",
                        false,
                        glib::ParamFlags::READWRITE,
                    ),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "pipeline" => {
                    let pipeline = value.get().unwrap();
                    self.pipeline.replace(pipeline);
                }
                "state" => {
                    let state = value.get().unwrap();
                    self.state.replace(state);
                }
                "is-readying" => {
                    let is_readying = value.get().unwrap();
                    self.is_readying.set(is_readying);
                }
                _ => unimplemented!(),
            }
        }

        fn property(&self, _obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "pipeline" => self.state.borrow().to_value(),
                "state" => self.state.borrow().to_value(),
                "is-readying" => self.is_readying.get().to_value(),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct KhaRecorder(ObjectSubclass<imp::KhaRecorder>);
}

impl KhaRecorder {
    pub fn new() -> Self {
        glib::Object::new::<Self>(&[]).expect("Failed to create KhaRecorder")
    }

    fn private(&self) -> &imp::KhaRecorder {
        &imp::KhaRecorder::from_instance(self)
    }

    fn pipeline(&self) -> gst::Pipeline {
        let pipeline = self.property("pipeline").unwrap();
        pipeline.get::<gst::Pipeline>().unwrap()
    }

    fn portal(&self) -> &KhaScreencastPortal {
        let imp = self.private();
        &imp.portal
    }

    fn settings(&self) -> &KhaSettings {
        let imp = self.private();
        &imp.settings
    }

    fn set_state(&self, state: RecorderState) {
        self.set_property("state", state)
            .expect("Failed to set recorder state");

        let pipeline = self.pipeline();

        let pipeline_state = match state {
            RecorderState::Null => gst::State::Null,
            RecorderState::Paused => gst::State::Paused,
            RecorderState::Playing => gst::State::Playing,
        };

        pipeline
            .set_state(pipeline_state)
            .expect("Failed to set pipeline state");

        log::info!("Pipeline set to {:?}", pipeline_state);
    }

    fn set_is_readying(&self, is_readying: bool) {
        self.set_property("is-readying", is_readying)
            .expect("Failed to set recorder is_readying");
    }

    fn build_pipeline(&self, stream: Stream) {
        let imp = self.private();

        let fd = stream.fd;
        let node_id = stream.node_id;

        println!("{}", fd);
        println!("{}", node_id);
        println!("{}", stream.screen.width);
        println!("{}", stream.screen.height);

        let pipeline_string = format!("pipewiresrc fd={} path={} do-timestamp=true keepalive-time=1000 resend-last=true ! videoconvert ! queue ! vp8enc max_quantizer=17 cpu-used=16 cq_level=13 deadline=1 static-threshold=100 keyframe-mode=disabled buffer-size=20000 threads=3 ! queue ! webmmux ! filesink location=/home/dave/test.webm", fd, node_id);
        let gst_pipeline = gst::parse_launch(&pipeline_string).expect("Failed to parse pipeline");
        let gst_pipeline = gst_pipeline
            .downcast::<gst::Pipeline>()
            .expect("Couldn't downcast pipeline");
        imp.pipeline.replace(Some(gst_pipeline));

        self.set_property("state", RecorderState::Playing).unwrap();
    }

    pub fn ready(&self) {
        self.set_is_readying(true);
        let is_show_pointer = self.settings().is_show_pointer();
        let is_selection_mode = self.settings().is_selection_mode();
        self.portal().open();

        log::debug!("is_show_pointer: {}", is_show_pointer);
        log::debug!("is_selection_mode: {}", is_selection_mode);
    }

    pub fn start(&self) {
        let record_bus = self
            .pipeline()
            .bus()
            .expect("Failed to get bus for pipeline");

        record_bus.add_watch_local(clone!(@weak self as obj => @default-return Continue(true), move |_, message: &gst::Message| {
            match message.view() {
                gst::MessageView::Eos(..) => {
                    obj.set_state(RecorderState::Null);
                },
                gst::MessageView::Error(error) => {
                    obj.set_state(RecorderState::Null);
                    log::warn!("{}", error.debug().unwrap());
                },
                _ => (),
            }

            Continue(true)
        })).unwrap();

        self.set_state(RecorderState::Playing);
    }

    pub fn pause(&self) {
        self.set_state(RecorderState::Paused);
    }

    pub fn resume(&self) {
        self.set_state(RecorderState::Playing);
    }

    pub fn stop(&self) {
        let eos_event = gst::event::Eos::new();
        self.pipeline().send_event(eos_event);
    }

    pub fn cancel(&self) {
        self.portal().close();
    }
}
