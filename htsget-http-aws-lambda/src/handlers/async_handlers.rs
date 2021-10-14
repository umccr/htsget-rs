use lambda_runtime::{ Context, Error };
use lambda_http::{ Body, Request, Response, IntoResponse };

// use htsget_http_core::Endpoint;
// use htsget_search::htsget::HtsGet;

pub async fn handle_lambda_request(req: Request, _: Context) -> Result<impl IntoResponse, Error> {
    // TODO: Route logic here for the different endpoints
    // /reads/{service-info}
    // /variants/{service-info}
    // Handle routes here perhaps? Using "path parameters" in lambda_http:
    // https://github.com/awslabs/aws-lambda-rust-runtime/blob/master/lambda-http/src/ext.rs#L10

    let path = req.uri().path();
    //let method = *req.method();

    match Some(path) {
      Some("/reads") => unimplemented!(),
      Some("/variants") => unimplemented!(),
      _ => Ok(Response::builder()
            .status(400)
            .body(Body::from("Error".to_string()))
            .expect("htsget error")
          )  
    }
}