//! The following module provides an implementation of the data and ticket servers using Axum.
//!

pub mod data;
pub mod ticket;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::error::Error::ServerError;
use crate::error::Result;
use crate::server::data::DataServer;
use crate::server::ticket::TicketServer;
use axum::extract::Request;
use axum::Router;
use htsget_config::config::advanced::auth::AuthConfig;
use htsget_config::config::advanced::cors::CorsConfig;
use htsget_config::config::service_info::ServiceInfo;
use htsget_config::tls::TlsServerConfig;
use htsget_config::types::Scheme;
use htsget_search::HtsGet;
use http::HeaderValue;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tower::Service;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders};
use tracing::trace;
use tracing::{error, warn};

/// Represents the axum app state.
#[derive(Debug, Clone)]
pub struct AppState<H: HtsGet> {
  pub(crate) htsget: H,
  pub(crate) service_info: ServiceInfo,
}

impl<H: HtsGet> AppState<H> {
  /// Create a new app state.
  pub fn new(htsget: H, service_info: ServiceInfo) -> Self {
    Self {
      htsget,
      service_info,
    }
  }
}

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
  cors_layer = cors.allow_origins().apply_mirror(
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
  cors_layer = cors.allow_origins().apply_mirror(
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

/// An axum server which should bind an address.
#[derive(Debug, Clone)]
pub struct BindServer {
  addr: SocketAddr,
  cert_key_pair: Option<TlsServerConfig>,
  scheme: Scheme,
  cors: CorsConfig,
  auth: Option<AuthConfig>,
}

impl BindServer {
  pub fn new(addr: SocketAddr, cors: CorsConfig, auth: Option<AuthConfig>) -> Self {
    Self {
      addr,
      cert_key_pair: None,
      scheme: Scheme::Http,
      cors,
      auth,
    }
  }

  pub fn new_with_tls(
    addr: SocketAddr,
    cors: CorsConfig,
    auth: Option<AuthConfig>,
    tls: TlsServerConfig,
  ) -> Self {
    Self {
      addr,
      cert_key_pair: Some(tls),
      scheme: Scheme::Https,
      cors,
      auth,
    }
  }

  /// Get the scheme this formatter is using - either HTTP or HTTPS.
  pub fn get_scheme(&self) -> &Scheme {
    &self.scheme
  }

  /// Eagerly bind the address by returning a `Server`. This function also updates the
  /// address to the actual bound address, and replaces the cert_key_pair with None.
  pub async fn bind_server(&mut self) -> Result<Server> {
    let server = Server::bind_addr(self.addr, self.cert_key_pair.take()).await?;
    self.addr = server.local_addr()?;

    Ok(server)
  }

  /// Eagerly bind the address by returning a `DataServer`.
  pub async fn bind_data_server(&mut self) -> Result<DataServer> {
    let server = self.bind_server().await?;

    Ok(DataServer::new(
      server,
      self.cors.clone(),
      self.auth.clone(),
    ))
  }

  /// Eagerly bind the address by returning a `TicketServer`.
  pub async fn bind_ticket_server<H>(
    &mut self,
    htsget: H,
    service_info: ServiceInfo,
  ) -> Result<TicketServer<H>>
  where
    H: HtsGet + Clone + Send + Sync + 'static,
  {
    let server = self.bind_server().await?;

    Ok(TicketServer::new(
      server,
      htsget,
      service_info,
      self.cors.clone(),
      self.auth.clone(),
    ))
  }

  /// Get the [SocketAddr] of this formatter.
  pub fn get_addr(&self) -> SocketAddr {
    self.addr
  }
}

/// An Axum server.
#[derive(Debug)]
pub struct Server {
  listener: TcpListener,
  cert_key_pair: Option<TlsServerConfig>,
}

impl Server {
  /// Eagerly bind the address for use with the server, returning any errors.
  pub async fn bind_addr(
    addr: SocketAddr,
    cert_key_pair: Option<TlsServerConfig>,
  ) -> Result<Server> {
    let listener = TcpListener::bind(addr).await?;

    Ok(Self {
      listener,
      cert_key_pair,
    })
  }

  /// Run the actual server, using the router, key and certificate.
  pub async fn serve(self, app: Router) -> Result<()> {
    match self.cert_key_pair {
      None => axum::serve(self.listener, app)
        .await
        .map_err(|err| ServerError(err.to_string())),
      Some(tls) => {
        let tls_acceptor = TlsAcceptor::from(Arc::new(tls.into_inner()));

        loop {
          let tower_service = app.clone();
          let tls_acceptor = tls_acceptor.clone();

          trace!("accepting connection");
          let (cnx, addr) = self.listener.accept().await?;

          tokio::spawn(async move {
            let Ok(stream) = tls_acceptor.accept(cnx).await else {
              error!("error during tls handshake connection from {}", addr);
              return;
            };

            let stream = TokioIo::new(stream);
            let hyper_service =
              service_fn(move |request: Request<Incoming>| tower_service.clone().call(request));

            let ret = Builder::new(TokioExecutor::new())
              .serve_connection_with_upgrades(stream, hyper_service)
              .await;

            if let Err(err) = ret {
              warn!("error serving connection from {}: {}", addr, err);
            }
          });
        }
      }
    }
  }

  /// Get the local address the server has bound to.
  pub fn local_addr(&self) -> Result<SocketAddr> {
    Ok(self.listener.local_addr()?)
  }
}
