# grpc-quickfix-gw
grpcurl -plaintext -d '{"message": "Hello, server!"}' localhost:50051 fantasy.ExampleService.UnaryCall

grpcurl -plaintext -d '{"message": "Stream request"}' localhost:50051 fantasy.ExampleService.ServerStream

grpcurl -plaintext -d @ localhost:50051 fantasy.ExampleService.ClientStream <<EOM
{"message": "Message 1"}
{"message": "Message 2"}
{"message": "Message 3"}
EOM

grpcurl -plaintext -d @ localhost:50051 fantasy.ExampleService.BidiStream <<EOM
{"message": "Message A"}
{"message": "Message B"}
{"message": "Message C"}
EOM
