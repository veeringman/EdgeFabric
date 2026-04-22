//! Generated protobuf types and gRPC service definitions.

pub mod device {
    tonic::include_proto!("edgefabric.device.v1");
}

pub mod session {
    tonic::include_proto!("edgefabric.session.v1");
}

pub mod test {
    tonic::include_proto!("edgefabric.test.v1");
}

pub mod agent {
    tonic::include_proto!("edgefabric.agent.v1");
}

/// File descriptor set for gRPC server reflection.
pub const FILE_DESCRIPTOR_SET: &[u8] =
    tonic::include_file_descriptor_set!("edgefabric_descriptor");
