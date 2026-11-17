// Modified by ReallyMe Technologies Limited.
use crate::google::protobuf::__buffa::oneof::value::Kind;
use crate::google::protobuf::{Duration, ListValue, Struct, Timestamp, Value};
use zeroize::Zeroize;

impl Zeroize for Timestamp {
    fn zeroize(&mut self) {
        self.seconds.zeroize();
        self.nanos.zeroize();
        self.__buffa_unknown_fields.zeroize();
    }
}
impl Zeroize for Duration {
    fn zeroize(&mut self) {
        self.seconds.zeroize();
        self.nanos.zeroize();
        self.__buffa_unknown_fields.zeroize();
    }
}
impl Zeroize for Kind {
    fn zeroize(&mut self) {
        match self {
            Kind::NullValue(value) => value.zeroize(),
            Kind::NumberValue(value) => value.zeroize(),
            Kind::StringValue(value) => value.zeroize(),
            Kind::BoolValue(value) => value.zeroize(),
            Kind::StructValue(value) => value.zeroize(),
            Kind::ListValue(value) => value.zeroize(),
        }
    }
}
impl Zeroize for Value {
    fn zeroize(&mut self) {
        self.kind.zeroize();
        self.__buffa_unknown_fields.zeroize();
    }
}
impl Zeroize for Struct {
    fn zeroize(&mut self) {
        // Wipe inline numeric values while still in their hash-table slots.
        for value in self.fields.values_mut() {
            value.zeroize();
        }
        for (mut key, mut value) in self.fields.drain() {
            key.zeroize();
            value.zeroize();
        }
        self.__buffa_unknown_fields.zeroize();
    }
}
impl Zeroize for ListValue {
    fn zeroize(&mut self) {
        self.values.zeroize();
        self.__buffa_unknown_fields.zeroize();
    }
}

impl Zeroize for crate::google::protobuf::NullValue {
    fn zeroize(&mut self) {
        buffa::sensitive::reset_copy(self);
    }
}
