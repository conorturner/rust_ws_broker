.PHONY: build release docker-build

tag = 726426517603.dkr.ecr.eu-west-2.amazonaws.com/rust-app:0.2.13

build:
	cargo build

release:
	cargo build --release

docker-build:
	docker build -t $(tag) .

docker-push: docker-build
	docker push $(tag)

run-a:
	PEER_DISCOVERY_ADDRESS=localhost cargo run

run-b:
	PEER_DISCOVERY_ADDRESS=localhost PORT=8081 cargo run

docker-run:
	docker run --network host -d -e PEER_DISCOVERY_ADDRESS=localhost -e PORT=8081 $(tag)

deploy:
	aws --region eu-west-2 cloudformation deploy --template-file cloudformation.yml --stack-name rust-test --capabilities CAPABILITY_NAMED_IAM \
		--parameter-overrides VPC=vpc-d2274dba SubnetA=subnet-d5e82799 SubnetB=subnet-dd13aaa7 \
		Certificate=arn:aws:acm:eu-west-2:726426517603:certificate/5633dbf0-1de6-4b09-9d5b-99a7f233cc80 \
		Image=$(tag) \
		Subdomain=rust.dev HostedZoneName=voolu.io ContainerPort=8080 ServiceName=rust \
		AutoScalingTargetValue=10

