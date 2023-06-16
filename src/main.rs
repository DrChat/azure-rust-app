#![feature(decl_macro, io_error_other)]
#[macro_use]
extern crate rocket;

use azure_core::auth::TokenCredential;
use azure_identity::ImdsManagedIdentityCredential;

use rocket::{
    form::{Form, FromForm},
    fs::FileServer,
};
use rocket_dyn_templates::{context, Template};

mod api;

#[derive(Debug, FromForm)]
#[allow(dead_code)]
struct Submit<'v> {
    #[field(validate = len(1..))]
    name: &'v str,
}

#[post("/hello", data = "<form>")]
fn hello(form: Form<Submit<'_>>) -> Template {
    Template::render(
        "hello",
        context! {
            name: form.name,
        },
    )
}

#[get("/")]
async fn index() -> Template {
    let creds = ImdsManagedIdentityCredential::default();
    let resp = creds.get_token("https://management.azure.com").await;

    let ident = match resp {
        Ok(_t) => format!("authenticated"),
        Err(e) => format!("unable to authenticate: {e:#}"),
    };

    Template::render(
        "index",
        context! {
            ident: ident
        },
    )
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/static", FileServer::from("static"))
        .mount("/", routes![index, hello])
        .mount("/api", api::routes())
        .attach(Template::fairing())
}
