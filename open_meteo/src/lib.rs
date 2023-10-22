use async_trait::async_trait;
use chrono::{format::ParseErrorKind, DateTime, TimeZone};
use derive_builder::Builder;
use reqwest::Client;
use serde::Deserialize;
use std::fmt::Display;

#[derive(Builder)]
pub struct ForecastRequest {
    past_days: u8,
    future_days: u8,
    place_name: String,
}

impl ForecastRequest {
    pub fn city_only(city: &str) -> Result<Self, String> {
        ForecastRequestBuilder::default()
            .past_days(0)
            .future_days(2)
            .place_name(city.to_owned())
            .build()
            .map_err(|e| e.to_string())
    }

    pub fn city_specific_day(city: &str, day: u8) -> Result<Self, String> {
        if day > 15 {
            return Err(format!(
                "Date is {day} days in the future, 15 days is the furthest supported!"
            ));
        }

        ForecastRequestBuilder::default()
            .past_days(0)
            .future_days(day + 1)
            .place_name(city.to_owned())
            .build()
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
pub trait ForecastClient {
    async fn weather_forecast(&self, request: &ForecastRequest) -> Result<Meteo, String>;
}

pub struct ReqwestForecastClient {
    client: Client,
    geocoding_root_url: String,
    forecast_root_url: String,
}

impl ReqwestForecastClient {
    pub fn new(client: &Client, geocoding_root_url: String, forecast_root_url: String) -> Self {
        Self {
            forecast_root_url,
            geocoding_root_url,
            client: client.clone(),
        }
    }

    async fn geolocalise(&self, place_name: &str) -> Result<Geolocalisation, String> {
        let root = self.geocoding_root_url.as_str();
        let url = reqwest::Url::parse_with_params(
            format!("{root}/v1/search?format=json&count=100").as_str(),
            &[("name", place_name), ("count", "1"), ("language", "it")],
        )
        .map_err(|err| format!("{err}"))?;

        let request = self
            .client
            .get(url)
            .build()
            .map_err(|err| format!("{err}"))?;

        self.client
            .execute(request)
            .await
            .map_err(|err| format!("{err}"))?
            .json::<Geocoding>()
            .await
            .map_err(|err| format!("{err}"))?
            .results
            .into_iter()
            .nth(0)
            .ok_or(format!("'{place_name}' without hits"))?
            .try_into()
    }
}

#[async_trait]
impl ForecastClient for ReqwestForecastClient {
    async fn weather_forecast(&self, request: &ForecastRequest) -> Result<Meteo, String> {
        let root = self.forecast_root_url.as_str();
        let geo = self.geolocalise(&request.place_name).await?;

        let url = reqwest::Url::parse_with_params(
            format!("{root}/v1/forecast").as_str(), 
            &[
                ("latitude", &geo.latitude.to_string()), 
                ("longitude", &geo.longitude.to_string()), 
                ("hourly", &"temperature_2m,precipitation_probability,precipitation,windspeed_10m,winddirection_10m".to_string()),
                ("timezone", &geo.timezone.to_string()),
                ("past_days", &request.past_days.to_string()),
                ("forecast_days", &request.future_days.to_string()),
            ]
        ).map_err(|err| format!("{err}"))?;

        let request = self
            .client
            .get(url)
            .build()
            .map_err(|err| format!("{err}"))?;

        let forecast = self
            .client
            .execute(request)
            .await
            .map_err(|err| format!("{err}"))?
            .json::<Forecast>()
            .await
            .map_err(|err| format!("{err}"))?;
        (forecast, geo).try_into()
    }
}

#[derive(Deserialize)]
struct Geocoding {
    pub results: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub country: String,
    pub admin1: Option<String>,
    pub admin2: Option<String>,
    pub admin3: Option<String>,
    pub admin4: Option<String>,
}

impl Hit {
    pub fn where_is_placed(&self) -> String {
        vec![
            Some(&self.country),
            self.admin1.as_ref(),
            self.admin2.as_ref(),
            self.admin3.as_ref(),
            self.admin4.as_ref(),
        ]
        .into_iter()
        .flat_map(|x| x.into_iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
    }
}

impl TryFrom<Hit> for Geolocalisation {
    type Error = String;

    fn try_from(value: Hit) -> Result<Self, Self::Error> {
        Ok(Geolocalisation {
            description: value.where_is_placed(),
            name: value.name,
            latitude: value.latitude,
            longitude: value.longitude,
            timezone: value
                .timezone
                .parse()
                .map_err(|err: Self::Error| err.to_string())?,
        })
    }
}

struct Geolocalisation {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: chrono_tz::Tz,
    pub description: String,
}

#[derive(Deserialize)]
struct Forecast {
    pub timezone: String,
    pub hourly_units: HourlyUnits,
    pub hourly: Hourly,
}

#[derive(Deserialize)]
struct HourlyUnits {
    pub temperature_2m: String,
    pub precipitation: String,
    pub precipitation_probability: String,
    pub windspeed_10m: String,
    pub winddirection_10m: String,
}

#[derive(Deserialize)]
struct Hourly {
    pub time: Vec<String>,
    pub temperature_2m: Vec<Option<f64>>,
    pub precipitation: Vec<Option<f64>>,
    pub precipitation_probability: Vec<Option<f64>>,
    pub windspeed_10m: Vec<Option<f64>>,
    pub winddirection_10m: Vec<Option<f64>>,
}

impl TryFrom<(Forecast, Geolocalisation)> for Meteo {
    type Error = String;

    #[allow(deprecated)]
    fn try_from((value, geo): (Forecast, Geolocalisation)) -> Result<Self, Self::Error> {
        let utc = &chrono::Utc;
        let timezone = &value.timezone;
        let tz: chrono_tz::Tz = value
            .timezone
            .parse()
            .map_err(|_| format!("Unparseable timezone {timezone}"))?;

        let temperature_2m = value.hourly.temperature_2m;
        let t2m_unit = value.hourly_units.temperature_2m;

        let precipitation = value.hourly.precipitation;
        let p_unit = value.hourly_units.precipitation;

        let precipitation_probability = value.hourly.precipitation_probability;
        let pp_unit = value.hourly_units.precipitation_probability;

        let windspeed_10m = value.hourly.windspeed_10m;
        let w10m_unit = value.hourly_units.windspeed_10m;

        let winddirection_10m = value.hourly.winddirection_10m;
        let wd10m_unit = value.hourly_units.winddirection_10m;
        let mut result: Vec<Weather> = vec![];
        for (i, item) in value.hourly.time.iter().enumerate() {
            let date = match tz.datetime_from_str(&item.to_string(), "%Y-%m-%dT%H:%M") {
                // (2023-10-29T02:00): input is not enough for unique date and time
                // This is due to the fact that daylight saving time happens at that moment.
                // Time is moved back 1h => we have 1h "overlap" between the two different time offsets (+2 and +1)
                Err(parse) if parse.kind().eq(&ParseErrorKind::NotEnough) => continue,
                d => d,
            };
            let point = Weather {
                time: date
                    .map_err(|e| format!("Unable to parse date ({}): {e}", &item))?
                    .with_timezone(utc),
                precipitation: HumanReadableMeasure(
                    precipitation[i].unwrap_or(0f64),
                    p_unit.to_owned(),
                ),
                precipitation_probability: HumanReadableMeasure(
                    precipitation_probability[i].unwrap_or(0f64),
                    pp_unit.to_owned(),
                ),
                temperature_2m: HumanReadableMeasure(
                    temperature_2m[i].unwrap_or(0f64),
                    t2m_unit.to_owned(),
                ),
                windspeed_10m: HumanReadableMeasure(
                    windspeed_10m[i].unwrap_or(0f64),
                    w10m_unit.to_owned(),
                ),
                winddirection_10m: HumanReadableMeasure(
                    winddirection_10m[i].unwrap_or(0f64),
                    wd10m_unit.to_owned(),
                ),
            };
            result.push(point)
        }

        Ok(Meteo {
            city_name: geo.name,
            city_description: geo.description,
            time_series: result,
        })
    }
}

pub struct Meteo {
    pub city_name: String,
    pub city_description: String,
    pub time_series: Vec<Weather>,
}
impl Display for Meteo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{} ({})\n", self.city_name, self.city_description))?;
        for ele in &self.time_series {
            f.write_str(&format!("{ele}\n"))?;
        }
        Ok(())
    }
}

pub struct Weather {
    pub time: DateTime<chrono::Utc>,
    pub temperature_2m: HumanReadableMeasure,
    pub precipitation: HumanReadableMeasure,
    pub precipitation_probability: HumanReadableMeasure,
    pub windspeed_10m: HumanReadableMeasure,
    pub winddirection_10m: HumanReadableMeasure,
}

impl Display for Weather {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = &self
            .time
            .with_timezone(&chrono_tz::Europe::Rome)
            .format("%H:%M");

        let temp = &self.temperature_2m;
        let prec = &self.precipitation;
        let prob = &self.precipitation_probability;
        let wind = &self.windspeed_10m;
        let wind_dir = &self.winddirection_10m;

        f.write_str(&format!(
            "{time} -> 🌡️{temp} - 🌧️{prec}({prob}) - 💨{wind}({wind_dir})"
        ))
    }
}

pub struct HumanReadableMeasure(f64, String);

impl HumanReadableMeasure {
    pub fn number(&self) -> f64 {
        self.0
    }

    pub fn unit(&self) -> &str {
        &self.1
    }
}

impl Display for HumanReadableMeasure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let num = self.0;
        let unit = &self.1;
        f.write_str(&format!("{num}{unit}"))
    }
}
