use image::{DynamicImage, GenericImageView, Pixel, RgbImage};

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

    pub async fn simple_query(&self, query: &str) -> anyhow::Result<WolframAlphaSimpleResult> {
        let response = self
            .reqwest
            .get("https://api.wolframalpha.com/v1/simple")
            .query(&vec![
                ("i", query),
                ("units", "metric"),
                ("appid", &self.api_key),
                ("layout", "labelbar"),
            ])
            .send()
            .await?;

        Ok(WolframAlphaSimpleResult {
            img: response.bytes().await?.to_vec(),
        })
    }

    pub async fn short_answer(&self, query: &str) -> anyhow::Result<String> {
        let response = self
            .reqwest
            .get("https://api.wolframalpha.com/v1/result")
            .query(&vec![("i", query), ("appid", &self.api_key)])
            .send()
            .await?;

        Ok(String::from_utf8(response.bytes().await?.to_vec())?)
    }
}

pub struct WolframAlphaSimpleResult {
    pub img: Vec<u8>,
}

impl WolframAlphaSimpleResult {
    pub fn slice_image(&self) -> anyhow::Result<Vec<DynamicImage>> {
        let img = image::load_from_memory(&self.img)?;
        let mut slices: Vec<(u32, u32)> = Vec::new();
        let mut slice_top = 0;
        let mut in_marker = false;

        let column = (0..img.height())
            .map(|y| (y, img.get_pixel(1, y).to_rgb()))
            // Skip leading image
            .skip_while(|(_, px)| px.0 == [255; 3]);

        for (y, pixel) in column {
            // Begin of marker
            if !in_marker && pixel.0 != [255; 3] {
                in_marker = true;
                slices.push((slice_top, y - 1));
                slice_top = y;
            }
            // end of marker
            else if in_marker && pixel.0 == [255; 3] {
                in_marker = false;
            }
        }
        if slice_top < img.height() - 1 {
            slices.push((slice_top, img.height() - 1));
        }

        let mut image_slices = vec![];
        for (start, end) in slices {
            image_slices.push(img.crop_imm(0, start, img.width(), end - start + 1));
        }

        Ok(image_slices)
    }

    pub fn group_images(images: Vec<DynamicImage>, max_height: u32) -> Vec<RgbImage> {
        let mut groups = Vec::new();
        let mut current_group = Vec::new();
        let mut current_height = 0;

        for img in images {
            if current_height + img.height() > max_height && !current_group.is_empty() {
                groups.push(current_group);
                current_group = Vec::new();
                current_height = 0;
            }
            current_height += img.height();
            current_group.push(img);
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        groups
            .into_iter()
            .map(|group| {
                let height: u32 = group.iter().map(|img| img.height()).sum();
                let mut img = RgbImage::new(group[0].width(), height);
                let mut current_y = 0;
                for slice in group {
                    for x in 0..slice.width() {
                        for y in 0..slice.height() {
                            img.put_pixel(x, y + current_y, slice.get_pixel(x, y).to_rgb());
                        }
                    }
                    current_y += slice.height();
                }

                img
            })
            .collect()
    }
}
