from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class PlaceholderCtx(_message.Message):
    __slots__ = ("session_key", "model_name", "metadata")
    class MetadataEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(
            self, key: _Optional[str] = ..., value: _Optional[str] = ...
        ) -> None: ...

    SESSION_KEY_FIELD_NUMBER: _ClassVar[int]
    MODEL_NAME_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    session_key: str
    model_name: str
    metadata: _containers.ScalarMap[str, str]
    def __init__(
        self,
        session_key: _Optional[str] = ...,
        model_name: _Optional[str] = ...,
        metadata: _Optional[_Mapping[str, str]] = ...,
    ) -> None: ...

class RenderRequest(_message.Message):
    __slots__ = ("template", "ctx", "max_depth")
    TEMPLATE_FIELD_NUMBER: _ClassVar[int]
    CTX_FIELD_NUMBER: _ClassVar[int]
    MAX_DEPTH_FIELD_NUMBER: _ClassVar[int]
    template: str
    ctx: PlaceholderCtx
    max_depth: int
    def __init__(
        self,
        template: _Optional[str] = ...,
        ctx: _Optional[_Union[PlaceholderCtx, _Mapping]] = ...,
        max_depth: _Optional[int] = ...,
    ) -> None: ...

class RenderResponse(_message.Message):
    __slots__ = ("rendered", "unresolved_keys", "error")
    RENDERED_FIELD_NUMBER: _ClassVar[int]
    UNRESOLVED_KEYS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    rendered: str
    unresolved_keys: _containers.RepeatedScalarFieldContainer[str]
    error: str
    def __init__(
        self,
        rendered: _Optional[str] = ...,
        unresolved_keys: _Optional[_Iterable[str]] = ...,
        error: _Optional[str] = ...,
    ) -> None: ...
