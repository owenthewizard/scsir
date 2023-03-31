#![allow(dead_code)]

use std::mem::size_of;

use modular_bitfield_msb::prelude::*;

use crate::{
    data_wrapper::{AnyType, FlexibleStruct},
    result_data::ResultData,
    Command, DataDirection, Scsi,
};

#[derive(Clone, Debug)]
pub struct GetStreamStatusCommand<'a> {
    interface: &'a Scsi,
    command_buffer: CommandBuffer,
    descriptor_length: u32,
}

#[derive(Debug)]
pub struct CommandResult {
    pub total_descripter_length: usize,
    pub number_of_open_streams: u16,
    pub stream_identifiers: Vec<u16>,
}

impl<'a> GetStreamStatusCommand<'a> {
    fn new(interface: &'a Scsi) -> Self {
        Self {
            interface,
            descriptor_length: 0,
            command_buffer: CommandBuffer::new()
                .with_operation_code(OPERATION_CODE)
                .with_service_action(SERVICE_ACTION),
        }
    }

    pub fn starting_stream_identifier(&mut self, value: u16) -> &mut Self {
        self.command_buffer.set_starting_stream_identifier(value);
        self
    }

    pub fn control(&mut self, value: u8) -> &mut Self {
        self.command_buffer.set_control(value);
        self
    }

    // descriptor length must be less than 268435455(0xFFF_FFFF), which is (0xFFFF_FFFF - 8) / 16
    pub fn descriptor_length(&mut self, value: u32) -> &mut Self {
        self.descriptor_length = value;
        self
    }

    pub fn issue(&mut self) -> crate::Result<CommandResult> {
        const MAX_DESCRIPTOR_LENGTH: usize =
            (u32::MAX as usize - size_of::<ParameterHeader>()) / size_of::<Descriptor>();
        if self.descriptor_length > MAX_DESCRIPTOR_LENGTH as u32 {
            return Err(
                crate::Error::ArgumentOutOfBounds(
                    format!(
                        "descriptor length is out of bounds. The maximum possible value is {}, but {} was provided.",
                        MAX_DESCRIPTOR_LENGTH,
                        self.descriptor_length)));
        }

        let temp = ThisCommand {
            command_buffer: self.command_buffer.with_allocation_length(
                size_of::<ParameterHeader>() as u32
                    + self.descriptor_length * size_of::<Descriptor>() as u32,
            ),
            max_descriptor_length: self.descriptor_length,
        };

        self.interface.issue(&temp)
    }
}

impl Scsi {
    pub fn get_stream_status(&self) -> GetStreamStatusCommand {
        GetStreamStatusCommand::new(self)
    }
}

const OPERATION_CODE: u8 = 0x9E;
const SERVICE_ACTION: u8 = 0x16;

#[bitfield]
#[derive(Clone, Copy, Debug)]
struct CommandBuffer {
    operation_code: B8,
    reserved_0: B3,
    service_action: B5,
    reserved_1: B16,
    starting_stream_identifier: B16,
    reserved_2: B32,
    allocation_length: B32,
    reserved_3: B8,
    control: B8,
}

#[bitfield]
#[derive(Clone, Copy)]
struct ParameterHeader {
    parameter_data_length: B32,
    reserved: B16,
    number_of_open_streams: B16,
}

#[bitfield]
#[derive(Clone, Copy)]
struct Descriptor {
    reserved_0: B16,
    stream_identifier: B16,
    reserved_1: B32,
}

struct ThisCommand {
    command_buffer: CommandBuffer,
    max_descriptor_length: u32,
}

impl Command for ThisCommand {
    type CommandBuffer = CommandBuffer;

    type DataBuffer = AnyType;

    type DataBufferWrapper = FlexibleStruct<ParameterHeader, Descriptor>;

    type ReturnType = crate::Result<CommandResult>;

    fn direction(&self) -> DataDirection {
        DataDirection::FromDevice
    }

    fn command(&self) -> Self::CommandBuffer {
        self.command_buffer
    }

    fn data(&self) -> Self::DataBufferWrapper {
        unsafe { FlexibleStruct::with_length(self.max_descriptor_length as usize) }
    }

    fn data_size(&self) -> u32 {
        self.max_descriptor_length * size_of::<Descriptor>() as u32
            + size_of::<ParameterHeader>() as u32
    }

    fn process_result(&self, result: ResultData<Self::DataBufferWrapper>) -> Self::ReturnType {
        result.check_ioctl_error()?;
        result.check_common_error()?;

        let data = result.data;
        let length = unsafe { data.body_as_ref() }.parameter_data_length();
        let length = (length as usize - size_of::<u64>()) / size_of::<Descriptor>();

        let mut stream_identifiers = vec![];
        for item in unsafe { &data.elements_as_slice()[..usize::min(length, data.length())] } {
            stream_identifiers.push(item.stream_identifier());
        }

        Ok(CommandResult {
            total_descripter_length: length,
            number_of_open_streams: unsafe { data.body_as_ref() }.number_of_open_streams(),
            stream_identifiers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    const COMMAND_LENGTH: usize = 16;
    const PARAMETER_HEADER_LENGTH: usize = 8;
    const DESCRIPTOR_LENGTH: usize = 8;

    #[test]
    fn layout_test() {
        assert_eq!(
            size_of::<CommandBuffer>(),
            COMMAND_LENGTH,
            concat!("Size of: ", stringify!(CommandBuffer))
        );

        assert_eq!(
            size_of::<ParameterHeader>(),
            PARAMETER_HEADER_LENGTH,
            concat!("Size of: ", stringify!(ParameterHeader))
        );

        assert_eq!(
            size_of::<Descriptor>(),
            DESCRIPTOR_LENGTH,
            concat!("Size of: ", stringify!(Descriptor))
        );
    }
}
