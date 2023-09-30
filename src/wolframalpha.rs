use serde::{Deserialize, Serialize};

const USEFUL_PODS: [&str; 6] = [
    "solution",
    "result",
    "biological properties",
    "image",
    "color swatch",
    "related colors",
];

fn is_useful_pod(pod_title: &str) -> bool {
    for useful_pod in USEFUL_PODS {
        if useful_pod.eq_ignore_ascii_case(pod_title) {
            return true;
        }
    }
    false
}

pub struct WolframAlpha {
    api_key: String,
    reqwest: reqwest::Client,
}

impl WolframAlpha {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            reqwest: reqwest::Client::new(),
        }
    }

    pub async fn query(&self, query: &str) -> anyhow::Result<WolframAlphaResult> {
        let result = self.get_response(query).await?;
        Ok(serde_json::from_str(&result)?)
    }

    async fn get_response(&self, query: &str) -> anyhow::Result<String> {
        let response = self
            .reqwest
            .get("https://api.wolframalpha.com/v2/query")
            .query(&vec![
                ("input", query),
                ("format", "image,plaintext"),
                ("output", "JSON"),
                ("appid", &self.api_key),
            ])
            .send()
            .await?;

        Ok(String::from_utf8(response.bytes().await?.to_vec())?)
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WolframAlphaResult {
    pub queryresult: Queryresult,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Queryresult {
    pub success: bool,
    pub error: bool,
    pub numpods: i64,
    pub timing: f64,
    pub pods: Vec<Pod>,
}

impl Queryresult {
    pub fn filtered_pods(&self) -> Vec<Pod> {
        self.pods
            .iter()
            .filter(|pod| is_useful_pod(&pod.title))
            .cloned()
            .collect()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    pub title: String,
    pub position: i64,
    pub error: bool,
    pub subpods: Vec<Subpod>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subpod {
    pub img: Img,
    pub plaintext: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Img {
    pub src: String,
    pub alt: String,
    pub title: String,
    pub width: i64,
    pub height: i64,
    #[serde(rename = "type")]
    pub type_field: String,
    pub colorinvertable: bool,
}
