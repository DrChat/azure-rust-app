#![feature(decl_macro)]
#[macro_use]
extern crate rocket;

use rocket::{
    form::{Form, FromForm},
    fs::FileServer,
};
use rocket_dyn_templates::{context, Template};

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
pub fn index() -> Template {
    Template::render("index", context! {})
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/static", FileServer::from("static"))
        .mount("/", routes![index, hello])
        .attach(Template::fairing())
}
