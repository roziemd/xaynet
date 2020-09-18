//! Sum2 message payloads.
//!
//! See the [message module] documentation since this is a private module anyways.
//!
//! [message module]: ../index.html

use std::ops::Range;

use anyhow::{anyhow, Context};

use crate::{
    crypto::ByteObject,
    mask::object::{serialization::MaskObjectBuffer, MaskMany, MaskObject, MaskOne},
    message::{
        traits::{FromBytes, ToBytes},
        utils::range,
        DecodeError,
    },
    ParticipantTaskSignature,
};

const SUM_SIGNATURE_RANGE: Range<usize> = range(0, ParticipantTaskSignature::LENGTH);

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
/// A wrapper around a buffer that contains a [`Sum2`] message.
///
/// It provides getters and setters to access the different fields of the message safely.
pub struct Sum2Buffer<T> {
    inner: T,
}

impl<T: AsRef<[u8]>> Sum2Buffer<T> {
    /// Performs bound checks for the various message fields on `bytes` and returns a new
    /// [`Sum2Buffer`].
    ///
    /// # Errors
    /// Fails if the `bytes` are smaller than a minimal-sized sum2 message buffer.
    pub fn new(bytes: T) -> Result<Self, DecodeError> {
        let buffer = Self { inner: bytes };
        buffer
            .check_buffer_length()
            .context("not a valid Sum2Buffer")?;
        Ok(buffer)
    }

    /// Returns a `Sum2Buffer` with the given `bytes` without performing bound checks.
    ///
    /// This means that accessing the message fields may panic.
    pub fn new_unchecked(bytes: T) -> Self {
        Self { inner: bytes }
    }

    /// Performs bound checks for the various message fields on this buffer.
    pub fn check_buffer_length(&self) -> Result<(), DecodeError> {
        let len = self.inner.as_ref().len();
        if len < SUM_SIGNATURE_RANGE.end {
            return Err(anyhow!(
                "invalid buffer length: {} < {}",
                len,
                SUM_SIGNATURE_RANGE.end
            ));
        }

        // Check the length of the model mask field
        let _ = MaskObjectBuffer::new(&self.inner.as_ref()[self.model_mask_offset()..])
            .context("invalid model mask field")?;

        // Check the length of the scalar mask field
        let _ = MaskObjectBuffer::new(&self.inner.as_ref()[self.scalar_mask_offset()..])
            .context("invalid scalar mask field")?;

        Ok(())
    }

    /// Gets the offset of the model mask field.
    fn model_mask_offset(&self) -> usize {
        SUM_SIGNATURE_RANGE.end
    }

    /// Gets the offset of the scalar mask field.
    fn scalar_mask_offset(&self) -> usize {
        let model_mask =
            MaskObjectBuffer::new_unchecked(&self.inner.as_ref()[self.model_mask_offset()..]);
        self.model_mask_offset() + model_mask.len()
    }
}

impl<T: AsRef<[u8]> + AsMut<[u8]>> Sum2Buffer<T> {
    /// Gets a mutable reference to the sum signature field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn sum_signature_mut(&mut self) -> &mut [u8] {
        &mut self.inner.as_mut()[SUM_SIGNATURE_RANGE]
    }

    /// Gets a mutable reference to the model mask field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn model_mask_mut(&mut self) -> &mut [u8] {
        let offset = self.model_mask_offset();
        &mut self.inner.as_mut()[offset..]
    }

    /// Gets a mutable reference to the scalar mask field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn scalar_mask_mut(&mut self) -> &mut [u8] {
        let offset = self.scalar_mask_offset();
        &mut self.inner.as_mut()[offset..]
    }
}

impl<'a, T: AsRef<[u8]> + ?Sized> Sum2Buffer<&'a T> {
    /// Gets a reference to the sum signature field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn sum_signature(&self) -> &'a [u8] {
        &self.inner.as_ref()[SUM_SIGNATURE_RANGE]
    }

    /// Gets a reference to the model mask field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn model_mask(&self) -> &'a [u8] {
        let offset = self.model_mask_offset();
        &self.inner.as_ref()[offset..]
    }

    /// Gets a reference to the scalar mask field.
    ///
    /// # Panics
    /// Accessing the field may panic if the buffer has not been checked before.
    pub fn scalar_mask(&self) -> &'a [u8] {
        let offset = self.scalar_mask_offset();
        &self.inner.as_ref()[offset..]
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
/// A high level representation of a sum2 message.
///
/// These messages are sent by sum participants during the sum2 phase.
pub struct Sum2 {
    /// The signature of the round seed and the word "sum".
    ///
    /// This is used to determine whether a participant is selected for the sum task.
    pub sum_signature: ParticipantTaskSignature,

    /// A model mask computed by the participant.
    pub model_mask: MaskObject,
}

// TODO ToBytes impl for MaskObject
impl ToBytes for Sum2 {
    fn buffer_length(&self) -> usize {
        SUM_SIGNATURE_RANGE.end
            + self.model_mask.vector.buffer_length()
            + self.model_mask.scalar.buffer_length()
    }

    fn to_bytes<T: AsMut<[u8]> + AsRef<[u8]>>(&self, buffer: &mut T) {
        let mut writer = Sum2Buffer::new_unchecked(buffer.as_mut());
        self.sum_signature.to_bytes(&mut writer.sum_signature_mut());
        self.model_mask
            .vector
            .to_bytes(&mut writer.model_mask_mut());
        self.model_mask
            .scalar
            .to_bytes(&mut writer.scalar_mask_mut());
    }
}

// TODO FromBytes impl for MaskObject
impl FromBytes for Sum2 {
    fn from_bytes<T: AsRef<[u8]>>(buffer: &T) -> Result<Self, DecodeError> {
        let reader = Sum2Buffer::new(buffer.as_ref())?;
        Ok(Self {
            sum_signature: ParticipantTaskSignature::from_bytes(&reader.sum_signature())
                .context("invalid sum signature")?,
            model_mask: MaskObject::new(
                MaskMany::from_bytes(&reader.model_mask()).context("invalid model mask")?,
                MaskOne::from_bytes(&reader.scalar_mask()).context("invalid scalar mask")?,
            ),
        })
    }
}

#[cfg(test)]
pub(in crate::message) mod tests_helpers {
    use super::*;
    use crate::{crypto::ByteObject, mask::object::MaskMany};

    pub fn signature() -> (ParticipantTaskSignature, Vec<u8>) {
        let bytes = vec![0x99; ParticipantTaskSignature::LENGTH];
        let signature = ParticipantTaskSignature::from_slice(&bytes[..]).unwrap();
        (signature, bytes)
    }

    pub fn mask() -> (MaskMany, Vec<u8>) {
        use crate::mask::object::serialization::tests::{bytes, object};
        (object(), bytes())
    }

    pub fn mask_1() -> (MaskMany, Vec<u8>) {
        use crate::mask::object::serialization::tests::{bytes_1, object_1};
        (object_1(), bytes_1())
    }

    pub fn sum2() -> (Sum2, Vec<u8>) {
        let mut bytes = signature().1;
        bytes.extend(mask().1);
        bytes.extend(mask_1().1);
        let sum2 = Sum2 {
            sum_signature: signature().0,
            model_mask: mask().0,
            scalar_mask: mask_1().0,
        };
        (sum2, bytes)
    }
}

#[cfg(test)]
pub(in crate::message) mod tests {
    pub(in crate::message) use super::tests_helpers as helpers;
    use super::*;

    #[test]
    fn buffer_read() {
        let bytes = helpers::sum2().1;
        let buffer = Sum2Buffer::new(&bytes).unwrap();
        assert_eq!(buffer.sum_signature(), &helpers::signature().1[..]);
        let expected = helpers::mask().1;
        assert_eq!(&buffer.model_mask()[..expected.len()], &expected[..]);
        assert_eq!(buffer.scalar_mask(), &helpers::mask_1().1[..]);
    }

    #[test]
    fn buffer_write() {
        let mut bytes = vec![0xff; 110];
        {
            let mut buffer = Sum2Buffer::new_unchecked(&mut bytes);
            buffer
                .sum_signature_mut()
                .copy_from_slice(&helpers::signature().1[..]);
            let expected = helpers::mask().1;
            buffer.model_mask_mut()[..expected.len()].copy_from_slice(&expected[..]);
            buffer
                .scalar_mask_mut()
                .copy_from_slice(&helpers::mask_1().1[..]);
        }
        assert_eq!(&bytes[..], &helpers::sum2().1[..]);
    }

    #[test]
    fn encode() {
        let (sum2, bytes) = helpers::sum2();
        assert_eq!(sum2.buffer_length(), bytes.len());

        let mut buf = vec![0xff; sum2.buffer_length()];
        sum2.to_bytes(&mut buf);
        assert_eq!(buf, bytes);
    }

    #[test]
    fn decode() {
        let (sum2, bytes) = helpers::sum2();
        let parsed = Sum2::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, sum2);
    }
}
