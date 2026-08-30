use axum::Router;
use std::borrow::Cow;
use tower_http::services::{ServeDir, ServeFile};

/// Serves a single page application: every path that does not resolve to a
/// file below `static_resources_location` falls back to `index_file`, so the
/// client side router can take over.
pub struct Spa {
    /// Path to the index file which is served when no static resource matches.
    index_file: Cow<'static, str>,
    /// Route prefix the static resources are mounted on.
    static_resources_mount: Cow<'static, str>,
    /// Directory the static resources are read from.
    static_resources_location: Cow<'static, str>,
}

impl Default for Spa {
    fn default() -> Self {
        Self::new("./index.html", "/", "./")
    }
}

impl Spa {
    pub fn new(
        index_file: impl Into<Cow<'static, str>>,
        static_resources_mount: impl Into<Cow<'static, str>>,
        static_resources_location: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            index_file: index_file.into(),
            static_resources_mount: static_resources_mount.into(),
            static_resources_location: static_resources_location.into(),
        }
    }

    pub fn finish(self) -> Router {
        // `fallback` rather than `not_found_service`: the latter rewrites the
        // status to 404, while actix served the index with 200 OK so the
        // client side router can resolve the route.
        let serve_dir = ServeDir::new(self.static_resources_location.as_ref())
            .fallback(ServeFile::new(self.index_file.as_ref()));

        let mount = self.static_resources_mount.as_ref();
        if mount == "/" {
            Router::new().fallback_service(serve_dir)
        } else {
            Router::new().nest_service(mount, serve_dir)
        }
    }
}
