//! Reusable TAPD Open API client and protocol helpers.

mod client;
mod error;
mod url_parser;
mod value;
mod workflow;

pub use client::{
    AttachmentUpload, ImageUpload, TapdClient, TapdClientBuilder, VerifiedAttachmentUpload,
    VerifiedImageUpload,
};
pub use error::{Error, Result};
pub use url_parser::{TapdResourceKind, TapdUrl};
pub use value::{field_string, record, records, value_as_string};
pub use workflow::{AppendField, Transition, TransitionPlan, plan_transition};

/// Ordered query/form parameter collection.
///
/// A vector is used instead of a map because some TAPD endpoints accept
/// repeated keys and because deterministic ordering helps request tests.
pub type Params = Vec<(String, String)>;
