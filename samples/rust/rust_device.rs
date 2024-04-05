// SPDX-License-Identifier: GPL-2.0

//! Rust device sample.

use kernel::{
    Module,
    miscdev,
    prelude::*,
    file::{File, Operations},
    sync::{Arc, ArcBorrow, smutex::Mutex},
    io_buffer::{IoBufferReader, IoBufferWriter}
};


module! {
    type: DeviceModule,
    name: "rust_dev",
    author: "David English",
    description: "Rust Device Linux Kernel Module",
    license: "GPL",
}

const BLOCK_SIZE : usize = 4096;

struct Device {
  data:  Mutex<Vec<Vec<u8>>>,
  cursor: Mutex<usize>
}

impl Device {
    fn try_new() -> Result<Self> {
      let set = Vec::<Vec<u8>>::try_with_capacity(BLOCK_SIZE)?;
      Ok(Self {
        data: Mutex::new(set),
        cursor: Mutex::new(0)
      })
    }


    fn find_block( &self, row: usize) -> Result<usize> {
        let mut dat = self.data.lock();
        if row >= dat.len() {
            let fill = row.saturating_sub(dat.len()) + 1;
                for _i in 0..fill {
                    match dat.try_push(Vec::<u8>::new()) {
                        Ok(_) => continue,
                        Err(_) => {
                            pr_err!("OOM creating row {}\n", dat.len());
                            return Err(ENOMEM)
                        }
                    }
                }
        }
        if dat[row].len() != BLOCK_SIZE {
            match dat[row].try_resize(BLOCK_SIZE, 0) {
                Ok(..) => Ok(BLOCK_SIZE),
                Err(..) => {
                    pr_err!("OOM while allocating {} bytes for block {}\n", BLOCK_SIZE, row);
                    Err(ENOMEM)
                }
            }
        } else {
            return Ok(BLOCK_SIZE);
        }
    }
}

#[vtable]
impl Operations for Device {

    type OpenData = Arc<Device>;
    type Data = Arc<Device>;

    fn open( data: & Self::Data, _file: &File) -> Result<Self::Data> {
        Ok(data.clone())
    }

    fn read(
        this: ArcBorrow<'_, Device>,
        _file: &File,
        user_buff: &mut impl IoBufferWriter,
        _offset: u64,
    ) -> Result<usize> {
        if user_buff.is_empty() { return Ok(0) }
        let total_offset;
        {
            let curr_pos = this.cursor.lock();
            let cast : u64 = (*curr_pos).try_into().unwrap();
            total_offset = _offset.checked_add(cast).unwrap();
        }
        let block_index = total_offset.checked_div(BLOCK_SIZE as u64).unwrap();
        let _rem = total_offset.checked_rem(BLOCK_SIZE as u64).unwrap();
        let row : usize = block_index.try_into()?;
        let block_offset : usize = _rem.try_into()?;
        match this.find_block(row) {
            Ok(bytes) => {
                let tot = user_buff.len().checked_add(block_offset).unwrap();
                let mut end = bytes;
                if tot < bytes { end = tot; }
                let dat = this.data.lock();
                user_buff.write_slice(& dat[row][block_offset..end])?;
                return Ok(end.saturating_sub(block_offset));
            },
            Err(err) => Err(err)
        }
    }

    fn write(
        this: ArcBorrow<'_, Device>,
        _file: &File,
        user_buff: &mut impl IoBufferReader,
        _offset: u64,
    ) -> Result<usize> {
        if user_buff.is_empty() { return Ok(0) }
        let total_offset;
        {
            let curr_pos = this.cursor.lock();
            let cast : u64 = (*curr_pos).try_into().unwrap();
            total_offset = _offset.checked_add(cast).unwrap();
        }

        let block_index = total_offset / BLOCK_SIZE as u64;
        let _rem = total_offset % BLOCK_SIZE as u64;
        let row : usize = block_index.try_into()?;
        let offset : usize = _rem.try_into()?;
        match this.find_block(row) {
            Ok(bytes) => {
                let mut vec = this.data.lock();
                let tot = user_buff.len().checked_add(offset).unwrap();
                let mut end = bytes;
                if tot < bytes { end = tot }
                user_buff.read_slice(&mut vec[row][offset..end])?;
                return Ok(end.saturating_sub(offset))
            },
            Err(err) => Err(err)
        }
    }


    fn release(_this: Arc<Device>, _: &File) {}
}

struct DeviceModule {
    _dev: Pin<Box<miscdev::Registration<Device>>>,
}

impl Module for DeviceModule {
    fn init(name: &'static CStr, _module: &'static ThisModule) -> Result<Self> {
        let dev = Arc::try_new(Device::try_new()?)?;
        let reg = miscdev::Registration::<Device>::new_pinned(fmt!("{name}"), dev)?;
            pr_debug!("REGISTERING {}\n", fmt!("{name}"));
        Ok(DeviceModule {
            _dev: reg,
        })

