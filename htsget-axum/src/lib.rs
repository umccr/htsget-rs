use htsget_config::config::cors::CorsConfig;
use http::HeaderValue;
use std::time::Duration;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders};

pub mod data_server;
pub mod error;

/// Configure cors, settings allowed methods, max age, allowed origins, and if credentials
/// are supported.
pub fn configure_cors(cors: CorsConfig) -> CorsLayer {
  let mut cors_layer = CorsLayer::new();

  cors_layer = cors.allow_origins().apply_any(
    |cors_layer| cors_layer.allow_origin(AllowOrigin::any()),
    cors_layer,
  );
  cors_layer = cors.allow_origins().apply_mirror(
    |cors_layer| cors_layer.allow_origin(AllowOrigin::mirror_request()),
    cors_layer,
  );
  cors_layer = cors.allow_origins().apply_list(
    |cors_layer, origins| {
      cors_layer.allow_origin(
        origins
          .iter()
          .map(|header| header.clone().into_inner())
          .collect::<Vec<HeaderValue>>(),
      )
    },
    cors_layer,
  );

  cors_layer = cors.allow_headers().apply_any(
    |cors_layer| cors_layer.allow_headers(AllowHeaders::mirror_request()),
    cors_layer,
  );
  cors_layer = cors.allow_headers().apply_list(
    |cors_layer, headers| cors_layer.allow_headers(headers.clone()),
    cors_layer,
  );

  cors_layer = cors.allow_methods().apply_any(
    |cors_layer| cors_layer.allow_methods(AllowMethods::mirror_request()),
    cors_layer,
  );
  cors_layer = cors.allow_methods().apply_list(
    |cors_layer, methods| cors_layer.allow_methods(methods.clone()),
    cors_layer,
  );

  cors_layer = cors.expose_headers().apply_any(
    |cors_layer| cors_layer.expose_headers(ExposeHeaders::any()),
    cors_layer,
  );
  cors_layer = cors.expose_headers().apply_list(
    |cors_layer, headers| cors_layer.expose_headers(headers.clone()),
    cors_layer,
  );

  cors_layer
    .allow_credentials(cors.allow_credentials())
    .max_age(Duration::from_secs(cors.max_age() as u64))
}
