use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Cached glyph information including atlas rect, bearing offsets, and LRU tracking
#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    /// Drawable glyph pixels in the atlas. Width/height are the actual uploaded
    /// bitmap size, not the internal allocation block size.
    pub rect: Rect,
    /// Internal block allocation used for eviction.
    pub allocated_rect: Rect,
    pub x_bearing: f32,
    pub y_bearing: f32,
    pub last_used: u64, // LRU timestamp
}

/// Cache key for glyphs including weight for variable font support
pub type GlyphCacheKey = (usize, u16, u32, u32); // (font_id, glyph_id, size_px, weight)

/// Block-based allocation unit
#[derive(Clone, Copy, Debug)]
struct Block {
    is_free: bool,
}

pub struct AtlasManager {
    block_size: u32,
    blocks_per_row: u32,
    blocks_per_col: u32,
    blocks: Vec<Block>,
    // Mapping from (FontID, GlyphID, FontSize, Weight) -> GlyphInfo
    glyph_cache: HashMap<GlyphCacheKey, GlyphInfo>,
    // Reverse mapping: block index -> glyph key (for eviction)
    block_to_glyph: HashMap<usize, GlyphCacheKey>,
    // Access counter for LRU
    access_counter: u64,
}

impl AtlasManager {
    pub fn new(width: u32, height: u32) -> Self {
        let block_size = 64; // Fixed block size
        let blocks_per_row = width / block_size;
        let blocks_per_col = height / block_size;
        let total_blocks = (blocks_per_row * blocks_per_col) as usize;

        let mut blocks = Vec::with_capacity(total_blocks);
        for _row in 0..blocks_per_col {
            for _col in 0..blocks_per_row {
                blocks.push(Block { is_free: true });
            }
        }

        Self {
            block_size,
            blocks_per_row,
            blocks_per_col,
            blocks,
            glyph_cache: HashMap::new(),
            block_to_glyph: HashMap::new(),
            access_counter: 0,
        }
    }

    /// Get glyph info with weight support and update LRU timestamp
    pub fn get_glyph_info_with_weight(
        &mut self,
        font_id: usize,
        glyph_id: u16,
        size_px: u32,
        weight: u32,
    ) -> Option<GlyphInfo> {
        let key = (font_id, glyph_id, size_px, weight);
        if let Some(info) = self.glyph_cache.get_mut(&key) {
            self.access_counter += 1;
            info.last_used = self.access_counter;
            Some(*info)
        } else {
            None
        }
    }

    /// Allocate space for a glyph, evicting LRU glyphs if necessary
    pub fn allocate(&mut self, width: u32, height: u32) -> Option<Rect> {
        // Calculate how many blocks we need
        let blocks_needed_x = (width + self.block_size - 1) / self.block_size;
        let blocks_needed_y = (height + self.block_size - 1) / self.block_size;

        // Try to find contiguous free blocks
        if let Some(rect) = self.find_free_blocks(blocks_needed_x, blocks_needed_y) {
            return Some(rect);
        }

        // No free space - evict LRU glyphs and retry
        let blocks_needed = (blocks_needed_x * blocks_needed_y) as usize;
        self.evict_lru(blocks_needed);

        // Retry allocation
        self.find_free_blocks(blocks_needed_x, blocks_needed_y)
    }

    /// Find contiguous free blocks and mark them as used
    fn find_free_blocks(&mut self, blocks_x: u32, blocks_y: u32) -> Option<Rect> {
        for start_row in 0..=(self.blocks_per_col - blocks_y) {
            for start_col in 0..=(self.blocks_per_row - blocks_x) {
                // Check if all required blocks are free
                let mut all_free = true;
                for dy in 0..blocks_y {
                    for dx in 0..blocks_x {
                        let idx =
                            ((start_row + dy) * self.blocks_per_row + (start_col + dx)) as usize;
                        if !self.blocks[idx].is_free {
                            all_free = false;
                            break;
                        }
                    }
                    if !all_free {
                        break;
                    }
                }

                if all_free {
                    // Mark blocks as used
                    for dy in 0..blocks_y {
                        for dx in 0..blocks_x {
                            let idx = ((start_row + dy) * self.blocks_per_row + (start_col + dx))
                                as usize;
                            self.blocks[idx].is_free = false;
                        }
                    }

                    return Some(Rect {
                        x: start_col * self.block_size,
                        y: start_row * self.block_size,
                        width: blocks_x * self.block_size,
                        height: blocks_y * self.block_size,
                    });
                }
            }
        }
        None
    }

    /// Evict least recently used glyphs to free at least `needed_blocks` blocks
    fn evict_lru(&mut self, needed_blocks: usize) {
        // Collect all glyphs sorted by last_used (oldest first)
        let mut glyphs: Vec<(GlyphCacheKey, u64)> = self
            .glyph_cache
            .iter()
            .map(|(k, v)| (*k, v.last_used))
            .collect();
        glyphs.sort_by_key(|(_, last_used)| *last_used);

        let mut freed_blocks = 0;

        for (key, _) in glyphs {
            if freed_blocks >= needed_blocks {
                break;
            }

            if let Some(info) = self.glyph_cache.remove(&key) {
                // Free the blocks used by this glyph.
                let allocated = info.allocated_rect;
                let blocks_x = (allocated.width + self.block_size - 1) / self.block_size;
                let blocks_y = (allocated.height + self.block_size - 1) / self.block_size;
                let start_col = allocated.x / self.block_size;
                let start_row = allocated.y / self.block_size;

                for dy in 0..blocks_y {
                    for dx in 0..blocks_x {
                        let idx =
                            ((start_row + dy) * self.blocks_per_row + (start_col + dx)) as usize;
                        if idx < self.blocks.len() {
                            self.blocks[idx].is_free = true;
                            self.block_to_glyph.remove(&idx);
                        }
                    }
                }

                freed_blocks += (blocks_x * blocks_y) as usize;
            }
        }
    }

    /// Cache glyph with weight support (for variable fonts)
    pub fn cache_glyph_with_weight(
        &mut self,
        font_id: usize,
        glyph_id: u16,
        size_px: u32,
        weight: u32,
        mut info: GlyphInfo,
    ) {
        self.access_counter += 1;
        info.last_used = self.access_counter;

        let key = (font_id, glyph_id, size_px, weight);

        // Store block -> glyph mapping for eviction
        let allocated = info.allocated_rect;
        let blocks_x = (allocated.width + self.block_size - 1) / self.block_size;
        let blocks_y = (allocated.height + self.block_size - 1) / self.block_size;
        let start_col = allocated.x / self.block_size;
        let start_row = allocated.y / self.block_size;

        for dy in 0..blocks_y {
            for dx in 0..blocks_x {
                let idx = ((start_row + dy) * self.blocks_per_row + (start_col + dx)) as usize;
                self.block_to_glyph.insert(idx, key);
            }
        }

        self.glyph_cache.insert(key, info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_rect_keeps_drawable_size_separate_from_allocation() {
        let mut atlas = AtlasManager::new(128, 128);
        let allocated = atlas.allocate(17, 23).expect("allocation");
        assert_eq!(allocated.width, 64);
        assert_eq!(allocated.height, 64);

        let info = GlyphInfo {
            rect: Rect {
                x: allocated.x,
                y: allocated.y,
                width: 17,
                height: 23,
            },
            allocated_rect: allocated,
            x_bearing: 0.0,
            y_bearing: 0.0,
            last_used: 0,
        };

        atlas.cache_glyph_with_weight(0, 42, 32, 400, info);
        let cached = atlas
            .get_glyph_info_with_weight(0, 42, 32, 400)
            .expect("cached glyph");

        assert_eq!(cached.rect.width, 17);
        assert_eq!(cached.rect.height, 23);
        assert_eq!(cached.allocated_rect.width, 64);
        assert_eq!(cached.allocated_rect.height, 64);
    }
}
