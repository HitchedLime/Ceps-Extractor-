use actix_web::body::BoxBody;
use actix_web::http::header::ContentType;
use actix_web::http::StatusCode;
use actix_web::{
     get, post,  web, App, HttpRequest, HttpResponse, HttpServer, Responder,
    ResponseError,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Display};
use std::sync::Mutex;
use substring::Substring;

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LoadItem {
    time: String,
    automatic_frequency_positive: f64,
    automatic_frequency_negative: f64,
}

fn crop_string(input: String, left_by: String, right_by: String) -> String {
    let result = input.split(&left_by).collect::<Vec<&str>>()[1]
        .split(&right_by)
        .collect::<Vec<&str>>()[0];
    return String::from(result);
}

fn get_data_from_xml(
    body: String,
    legend: HashMap<String, String>,
) -> Vec<HashMap<String, String>> {
    let data_start = body.find("<data>").unwrap() + 5;
    let data_end = body.find("</data>").unwrap() - 1;
    let data_content = body.substring(data_start, data_end);

    let mut items: Vec<HashMap<String, String>> = Vec::new();
    for data_entry in String::from(data_content).split("<item").skip(1) {
        let date = crop_string(
            String::from(data_entry),
            String::from("date=\""),
            String::from("\""),
        );

        let mut item = HashMap::<String, String>::new();

        item.insert("time".to_string(), String::from(date));
        for legend_key in legend.keys() {
            let needle = format!("{}=\"", legend_key);
            let data_key = legend.get(legend_key).unwrap();
            let data_value = crop_string(
                String::from(data_entry),
                needle.clone().to_string(),
                String::from("\""),
            );

            item.insert(data_key.to_string(), data_value.to_string());
        }

        items.push(item);
    }

    return items;
}

fn get_legend_from_xml(body: String) -> HashMap<String, String> {
    let legend_start = body.find("<series>").unwrap() + 7;
    let legend_end = body.find("</series>").unwrap() - 1;
    let legend_content = body.substring(legend_start, legend_end);

    let mut legend = HashMap::<String, String>::new();
    for legend_entry in String::from(legend_content).split("<serie").skip(1) {
        let key = crop_string(
            String::from(legend_entry),
            String::from("id=\""),
            String::from("\""),
        );
        let value = crop_string(
            String::from(legend_entry),
            String::from("name=\""),
            String::from("\""),
        );

        legend.insert(key.to_string(), value.to_string());
    }

    return legend;
}

fn get_load_items_from_data(data: Vec<HashMap<String, String>>) -> Vec<LoadItem> {
    let mut items = Vec::<LoadItem>::new();

    for item in data {
        if !item.contains_key("aFRR+ [MW]") || !item.contains_key("aFRR- [MW]") {
            continue;
        }

        let a_f_r_p = String::from(item.get("aFRR+ [MW]").unwrap());
        let a_f_r_n = String::from(item.get("aFRR- [MW]").unwrap());

        if a_f_r_p.len() == 0 || a_f_r_n.len() == 0 {
            continue;
        }

        items.push(LoadItem {
            time: String::from(item.get("time").unwrap()),
            automatic_frequency_negative: String::from(item.get("aFRR+ [MW]").unwrap())
                .parse::<f64>()
                .unwrap(),
            automatic_frequency_positive: String::from(item.get("aFRR- [MW]").unwrap())
                .parse::<f64>()
                .unwrap(),
        });
    }

    return items;
}



impl Responder for LoadItem{
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        let res_body = serde_json::to_string(&self).unwrap();

        // Create HttpResponse and set Content Type
        HttpResponse::Ok()
            .content_type(ContentType::json())
            .body(res_body)
    }
}

#[derive(Debug, Serialize)]
struct ErrNoId {
    id: u32,
    err: String,
}

// Implement ResponseError for ErrNoId
impl ResponseError for ErrNoId {
    fn status_code(&self) -> StatusCode {
        StatusCode::NOT_FOUND
    }

    fn error_response(&self) -> HttpResponse<BoxBody> {
        let body = serde_json::to_string(&self).unwrap();
        let res = HttpResponse::new(self.status_code());
        res.set_body(BoxBody::new(body))
    }
}

// Implement Display for ErrNoId
impl Display for ErrNoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

struct AppState {
    items: Mutex<Vec<LoadItem>>,
}


#[post("/LoadItem")]
async fn post_item(req: web::Json<LoadItem>, data: web::Data<AppState>) -> impl Responder {
    let new_item = LoadItem {
        automatic_frequency_negative: req.automatic_frequency_negative,

        automatic_frequency_positive: req.automatic_frequency_positive,
        time: String::from(&req.time),
    };

    let mut items = data.items.lock().unwrap();

    let response = serde_json::to_string(&new_item).unwrap();

    items.push(new_item);
    HttpResponse::Created()
        .content_type(ContentType::json())
        .body(response)
}


#[get("/LoadItem")]
async fn get_item(data: web::Data<AppState>) -> impl Responder {
    let items = data.items.lock().unwrap();

    let response = serde_json::to_string(&(*items)).unwrap();

    HttpResponse::Ok()
        .content_type(ContentType::json())
        .body(response)
}



#[actix_web::main]
async fn main() -> std::io::Result<()> {
    
    let body = format!("<soapenv:Envelope xmlns:cep=\"https://www.ceps.cz/CepsData/\" xmlns:soapenv=\"http://schemas.xmlsoap.org/soap/envelope/\"><soapenv:Header /><soapenv:Body><cep:AktivaceSVRvCR><cep:dateFrom>2022-07-26T09:00:00</cep:dateFrom><cep:dateTo>2022-07-27T09:00:00</cep:dateTo><cep:agregation>MI</cep:agregation><cep:function>AVG</cep:function><cep:param1>all</cep:param1></cep:AktivaceSVRvCR></soapenv:Body></soapenv:Envelope>");

    let client = reqwest::Client::new();
    let res = client
        .post("https://vip-prod-service-00-azapp.azurewebsites.net/_layouts/CepsData.asmx")
        .header("Content-Type", "text/xml")
        .body(body)
        .send()
        .await;
    let response_unwrap = res.unwrap().text();
    let parsed = response_unwrap.await.unwrap();
    let legend = get_legend_from_xml(parsed.clone());
    let data = get_data_from_xml(String::from(parsed), legend);
    let items = get_load_items_from_data(data);
    

    let app_state = web::Data::new(AppState {
        items: Mutex::new(items.clone()),
    });

    println!("Server starting at http://127.0.0.1:8000/LoadItem");
    HttpServer::new(move || {
        App::new()
            .app_data(app_state.clone())
            .service(post_item)
            .service(get_item)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await
}
