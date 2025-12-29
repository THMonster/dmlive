use anyhow::*;
use bytes::{BufMut, Bytes, BytesMut};

pub const MKV_HEADER: &'static [u8] = include_bytes!("header.mkv");

#[derive(Debug)]
pub struct MKVBlockGroup {
    block_group_id: u8, // 0xa0
    block_group_size: u32,
    block_id: u8, // 0xa1
    block_size: u32,
    block_track_number: u8, // 0x81
    block_relative_time: u16,
    block_header_flags: u8, // 0x00
    block_content: Vec<u8>,
    block_duration_id: u8,   // 0x9b
    block_duration_size: u8, // 0x84
    block_duration_content: u32,
}

#[derive(Debug)]
pub struct DMKVCluster {
    cluster_id: u32,
    cluster_size: u64,
    timestamp_id: u8,
    timestamp_size: u8,
    timestamp: u64,
    danmaku: Vec<MKVBlockGroup>,
}

impl DMKVCluster {
    pub fn new() -> Self {
        Self {
            cluster_id: 0x1f43b675,
            cluster_size: 10 as u64 | 0x0100_0000_0000_0000,
            timestamp_id: 0xe7,
            timestamp_size: 0x88,
            timestamp: 0,
            danmaku: Vec::new(),
        }
    }

    pub fn reset(&mut self, first_ts: u64) -> () {
        self.timestamp = first_ts;
        self.danmaku.clear();
        self.cluster_size = 10 as u64 | 0x0100_0000_0000_0000;
    }

    pub fn add_ass_block(&mut self, ts: u64, ass: Vec<u8>, speed: u64, track_number: u8) -> Result<()> {
        let ass_len = ass.len();
        let b = MKVBlockGroup {
            block_group_id: 0xa0,
            block_group_size: (ass_len + 15) as u32 | 0x1000_0000u32,
            block_id: 0xa1,
            block_size: (ass_len + 4) as u32 | 0x1000_0000u32,
            // block_track_number: 0x81,
            block_track_number: 0x80 | track_number,
            block_relative_time: ts.saturating_sub(self.timestamp) as u16,
            block_header_flags: 0x00,
            block_content: ass,
            block_duration_id: 0x9b,
            block_duration_size: 0x84,
            block_duration_content: speed as u32,
        };
        self.cluster_size += ass_len as u64 + 20;
        self.danmaku.push(b);
        Ok(())
    }

    pub fn write_to_bytes(&self) -> Bytes {
        let mut out = BytesMut::with_capacity(128000);
        out.put_u32(self.cluster_id);
        out.put_u64(self.cluster_size);
        out.put_u8(self.timestamp_id);
        out.put_u8(self.timestamp_size);
        out.put_u64(self.timestamp);
        for b in self.danmaku.iter() {
            out.put_u8(b.block_group_id);
            out.put_u32(b.block_group_size);
            out.put_u8(b.block_id);
            out.put_u32(b.block_size);
            out.put_u8(b.block_track_number);
            out.put_u16(b.block_relative_time);
            out.put_u8(b.block_header_flags);
            out.put_slice(&b.block_content);
            out.put_u8(b.block_duration_id);
            out.put_u8(b.block_duration_size);
            out.put_u32(b.block_duration_content);
        }
        out.freeze()
    }
}
