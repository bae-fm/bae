//! The images of a candidate's folder as its lightbox shows them, read in one
//! statement: every image in the order the scan stored them, the image the
//! lightbox shows, and the images before and after it.

use super::*;
use crate::folder_scanner::FileVersion;
use crate::{MappingImage, Neighbors};

/// Which candidate's folder images a lightbox shows, and the file id of the
/// image the person moved to last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderImagesRequest {
    pub candidate: crate::CandidateKey,
    pub focus: Option<String>,
}

/// The image a lightbox shows, and the images before and after it, which a
/// lone image lacks.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderImageFocus {
    pub image: MappingImage,
    pub neighbors: Option<Neighbors<String>>,
}

/// A candidate's folder images as its lightbox shows them, and the image it
/// shows, or none while the folder holds no image.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderImages {
    pub images: Vec<MappingImage>,
    pub focus: Option<FolderImageFocus>,
}

/// What a read of a lightbox delivered: the request it answered, and the
/// images. A read for no request delivers neither.
#[derive(Debug, Clone, PartialEq)]
pub struct FolderImagesSnapshot {
    pub request: Option<FolderImagesRequest>,
    pub images: Option<FolderImages>,
}

/// One row: the folder's images as a JSON array, and the image the lightbox
/// shows with the file ids before and after it, wrapping around the ends.
/// The lightbox shows the image `:focus` names while the folder holds it,
/// else the first.
fn folder_images_sql() -> String {
    let image = "json_object('id', i.id, 'name', i.file_name, 'path', hex(i.absolute_path), \
                 'size', i.size, 'modified_at_ns', i.modified_at_ns)";
    format!(
        "
WITH images AS MATERIALIZED (
    SELECT f.relative_path AS id, f.file_name, f.absolute_path, f.size, f.modified_at_ns,
           ROW_NUMBER() OVER (ORDER BY f.position) - 1 AS n, COUNT(*) OVER () AS total
    FROM scan_candidate_file AS f
    WHERE f.watched_folder_path =
              (SELECT watched_folder_path FROM scan_candidate WHERE path = :key LIMIT 1)
      AND f.candidate_path = :key AND f.role = 'artwork'
),
focused AS (SELECT n FROM images ORDER BY id IS :focus DESC, n LIMIT 1)
SELECT json_group_array(json({image}) ORDER BY i.n),
       CASE WHEN COUNT(i.n) > 0 THEN json_object(
           'image', json(MAX(CASE WHEN i.n = f.n THEN {image} END)),
           'neighbors', CASE WHEN COUNT(i.n) > 1 THEN json_object(
               'previous', MAX(CASE WHEN i.n = (f.n + i.total - 1) % i.total THEN i.id END),
               'next', MAX(CASE WHEN i.n = (f.n + 1) % i.total THEN i.id END)) END)
       END
FROM images AS i LEFT JOIN focused AS f"
    )
}

/// One image as the statement returns it.
#[derive(serde::Deserialize)]
struct ImageJson {
    id: String,
    name: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
    size: u64,
    modified_at_ns: i64,
}

impl ImageJson {
    fn image(self) -> MappingImage {
        MappingImage {
            file_id: self.id,
            name: self.name,
            file: FileVersion {
                path: self.path,
                size: self.size,
                modified_at_ns: self.modified_at_ns,
            },
        }
    }
}

/// The file ids before and after the shown image, as the statement returns
/// them.
#[derive(serde::Deserialize)]
struct NeighborsJson {
    previous: String,
    next: String,
}

/// The shown image, as the statement returns it.
#[derive(serde::Deserialize)]
struct FocusJson {
    image: ImageJson,
    neighbors: Option<NeighborsJson>,
}

/// The folder images `request`'s lightbox shows, as one read.
fn folder_images_on(
    sql: &impl coven::SqlRead,
    request: &FolderImagesRequest,
) -> Result<FolderImages, DbError> {
    let (images, focus) = sql.query_row(
        &folder_images_sql(),
        named_params! { ":key": &request.candidate, ":focus": &request.focus },
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
    )?;
    let images: Vec<ImageJson> = serde_json::from_str(&images)
        .map_err(|error| DbError::unreadable_json("a folder's images", &images, error))?;
    let focus = focus
        .map(|json| {
            serde_json::from_str::<FocusJson>(&json)
                .map_err(|error| DbError::unreadable_json("a lightbox's image", &json, error))
        })
        .transpose()?;
    Ok(FolderImages {
        images: images.into_iter().map(ImageJson::image).collect(),
        focus: focus.map(|focus| FolderImageFocus {
            image: focus.image.image(),
            neighbors: focus
                .neighbors
                .map(|neighbors| Neighbors { previous: neighbors.previous, next: neighbors.next }),
        }),
    })
}

impl ImportTables {
    /// Follow the images the lightbox of the candidate `initial` names
    /// shows, or nothing. A new request changes them on the same query, and
    /// coven reruns the query when a scan changes the folder's files.
    pub fn subscribe_folder_images(
        &self,
        initial: Option<FolderImagesRequest>,
    ) -> coven::ReconfigurableLiveQuery<Option<FolderImagesRequest>, Option<FolderImages>, DbError>
    {
        self.store.subscribe_reconfigurable(initial, |request, sql| match request {
            None => Ok(None),
            Some(request) => Ok(Some(folder_images_on(&sql, request)?)),
        })
    }
}

#[cfg(test)]
#[path = "folder_images_tests.rs"]
mod tests;
