//! The images of a candidate's folder as its lightbox shows them, read in one
//! statement: every image in the order the scan stored them, the image the
//! lightbox shows, and the images before and after it.

use super::*;
use crate::folder_scanner::FileVersion;
use crate::{MappingImage, Neighbors};
use bae_store::statement_row;

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

// One row: the images of the folder of the candidate `:key` as a JSON array,
// in the order the scan stored them, and the image the lightbox shows with the
// file ids before and after it, wrapping around the ends. The lightbox shows
// the image `:focus` names while the folder holds it, else the first.
sqlive_compose::read!(
    LightboxImages(key: crate::CandidateKey, focus: Option<String>) {
        struct LightboxImage { id: String, name: String, path: hex, size: u64, modified_at_ns: i64 }
        struct LightboxAround { previous: String, next: String }
        struct LightboxFocus { image: LightboxImage, neighbors: Option<LightboxAround> }
        images: json<Vec<LightboxImage>>,
        focus: json<Option<LightboxFocus>>
    },
    r#"
    WITH images AS MATERIALIZED (
        SELECT file.relative_path AS id, file.file_name, file.absolute_path, file.size,
               file.modified_at_ns,
               ROW_NUMBER() OVER (ORDER BY file.position) - 1 AS n, COUNT(*) OVER () AS total
        FROM @crate::tables::folder_scans::row_fragments::CandidateFiles(key: :key) AS file
        WHERE file.role = 'artwork'
    ),
    focused AS (SELECT n FROM images ORDER BY id IS :focus DESC, n LIMIT 1)
    SELECT json_group_array(json_object('id', i.id, 'name', i.file_name,
               'path', hex(i.absolute_path), 'size', i.size,
               'modified_at_ns', i.modified_at_ns) ORDER BY i.n) AS images,
           CASE WHEN COUNT(i.n) > 0 THEN json_object(
               'image', json(MAX(CASE WHEN i.n = f.n THEN json_object(
                   'id', i.id, 'name', i.file_name, 'path', hex(i.absolute_path),
                   'size', i.size, 'modified_at_ns', i.modified_at_ns) END)),
               'neighbors', CASE WHEN COUNT(i.n) > 1 THEN json_object(
                   'previous',
                   MAX(CASE WHEN i.n = (f.n + i.total - 1) % i.total THEN i.id END),
                   'next', MAX(CASE WHEN i.n = (f.n + 1) % i.total THEN i.id END)) END)
           END AS focus
    FROM images AS i LEFT JOIN focused AS f
"#
);

impl LightboxImage {
    fn image(self) -> Result<MappingImage, DbError> {
        Ok(MappingImage {
            file_id: self.id,
            name: self.name,
            file: FileVersion {
                path: path_column::decode(self.path)?,
                size: self.size,
                modified_at_ns: self.modified_at_ns,
            },
        })
    }
}

/// The folder images `request`'s lightbox shows, as one read.
fn folder_images_on(
    sql: &impl coven::SqlRead,
    request: &FolderImagesRequest,
) -> Result<FolderImages, DbError> {
    let row = statement_row(
        sql,
        &LightboxImagesParams { key: request.candidate.clone(), focus: request.focus.clone() },
    )?;
    Ok(FolderImages {
        images: row.images.into_iter().map(LightboxImage::image).collect::<Result<_, _>>()?,
        focus: row
            .focus
            .map(|focus| {
                Ok::<_, DbError>(FolderImageFocus {
                    image: focus.image.image()?,
                    neighbors: focus
                        .neighbors
                        .map(|around| Neighbors { previous: around.previous, next: around.next }),
                })
            })
            .transpose()?,
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
