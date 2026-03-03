use memmap2::Mmap;
use std::fs::File;

// memory-maps the file; caller holds the Mmap so chunk slices can borrow it
pub fn map_file(file: File) -> Mmap {
    let mmap = unsafe { Mmap::map(&file) }.unwrap();
    mmap.advise(memmap2::Advice::Sequential).unwrap();
    mmap
}

// splits input into chunks of either 512 chars or a new paragraph;
// if it splits before reaching a new paragraph it will drop a marker to contiue
pub fn split_chunks<'a>(data: &'a [u8], chunk_size: usize) -> Vec<&'a [u8]> {
    let chars_to_bytes = chunk_size * 4; // UTF-8 max 4 bytes per char

    let mut paragraphs: Vec<&[u8]> = Vec::new();

    let mut start: usize = 0;

    while start < data.len() {
        let mut end = start;

        while end < data.len()
            && end - start < chars_to_bytes
            && !(end - start >= 2 && data[end - 2..end] == *b"\n\n")
        {
            end += 1;
        }

        paragraphs.push(&data[start..end]);

        start = end;
    }

    paragraphs
}
