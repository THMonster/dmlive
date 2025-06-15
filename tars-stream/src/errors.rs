quick_error! {
    #[derive(Debug, PartialEq, Eq)]
    pub enum DecodeErr{
        NoEnoughDataErr{
            display("decoder: without enough data to read")
        }
        UnknownTarsTypeErr{
            display("decoder: unknown tars type")
        }
        TarsTagNotFoundErr{
            display("decoder: Tag Not Found")
        }
        MisMatchTarsTypeErr {
            display("decoder: mismatch type")
        }
        WrongSimpleListTarsTypeErr {
            display("decoder: wrong simple list type")
        }
        InvalidEnumValue {
            display("decoder: invalid enum value")
        }
        FieldNotFoundErr(desc: String) {
            display("{}", desc)
        }
        TypeNotFoundErr(desc: String) {
            display("{}", desc)
        }
        TupKeyNotFoundErr {
            display("decoder: Tup Key Not Found")
        }
        UnsupportTupVersionErr {
            display("decoder: Unsupport protocol version")
        }
    }
}

quick_error! {
    #[derive(Debug, PartialEq, Eq)]
    pub enum TarsTypeErr{
        DisMatchTarsTypeErr{
            display("tars_type: disMatch tars_type")
        }
    }
}

quick_error! {
    #[derive(Debug, PartialEq, Eq)]
    pub enum EncodeErr{
        TooBigTagErr{
            display("encoder: tag too big, max value is 255")
        }
        ConvertU8Err{
            display("encoder: cannot convert to u8")
        }
        DataTooBigErr {
            display("encoder: data bigger than 4294967295 bytes")
        }
        UnknownTarsTypeErr{
            display("encoder: unknown tars type")
        }
        UnsupportTupVersionErr {
            display("encoder:  Unsupport protocol version")
        }
    }
}
