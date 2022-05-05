import * as cdk from 'aws-cdk-lib';
import {Duration, Stack, StackProps} from 'aws-cdk-lib';
import {Construct} from 'constructs';
import {RustFunction} from 'rust.aws-cdk-lambda';
import {Architecture} from 'aws-cdk-lib/aws-lambda';
import * as apigw from 'aws-cdk-lib/aws-apigateway';
import {AuthorizationType} from 'aws-cdk-lib/aws-apigateway';
import * as iam from 'aws-cdk-lib/aws-iam';

export class ServerlessBioinformaticsStack extends Stack {
    constructor(scope: Construct, id: string, props?: StackProps) {
        super(scope, id, props);

        const stackName = id
        const mainLambdaBinName = id  // i.e. s3-rust-noodles-bam
        const apigwBinName = 'apigw'
        const bucketName = 'umccr-research-dev'

        const lambdaRole = new iam.Role(this, stackName + '-role', {
            assumedBy: new iam.ServicePrincipal('lambda.amazonaws.com'),
            description: 'Lambda execution role for ' + stackName,
        });

        const s3BucketPolicy = new iam.PolicyStatement({
            actions: ['s3:List*', 's3:Get*'],
            resources: ['arn:aws:s3:::*'],
        });

        lambdaRole.addManagedPolicy(iam.ManagedPolicy.fromAwsManagedPolicyName("service-role/AWSLambdaBasicExecutionRole"));
        lambdaRole.addToPolicy(s3BucketPolicy);

        let bamLambda = new RustFunction(this, stackName, {
            bin: mainLambdaBinName,
            memorySize: 128,
            // Increase the max timeout slightly
            timeout: Duration.seconds(10),
            environment: {
                BUCKET_NAME: bucketName,
            },
            // Useful so library logs show up in CloudWatch
            setupLogging: true,
            // Enable optional features and env variables at build (compile) time.
            //features: ['my-second-feature'],
            // buildEnvironment: {
            //     MY_BUILD_ENV_VAR: 'Testing 123.',
            // },
            architecture: Architecture.ARM_64,
            role: lambdaRole,
            target: "aarch64-unknown-linux-gnu"
        });

        let apiBamLambda = new RustFunction(this, stackName + "-" + apigwBinName, {
            bin: apigwBinName,
            memorySize: 128,
            // Increase the max timeout slightly
            timeout: Duration.seconds(10),
            environment: {
                BUCKET_NAME: bucketName,
            },
            // Useful so library logs show up in CloudWatch
            setupLogging: true,
            // Enable optional features and env variables at build (compile) time.
            //features: ['my-second-feature'],
            // buildEnvironment: {
            //     MY_BUILD_ENV_VAR: 'Testing 123.',
            // },
            architecture: Architecture.ARM_64,
            role: lambdaRole,
            target: "aarch64-unknown-linux-gnu"
        });

        const api = new apigw.LambdaRestApi(this, stackName + '-api', {
            handler: apiBamLambda,
            proxy: true,
            defaultMethodOptions: {
                authorizationType: AuthorizationType.IAM,
            }
        });

        new cdk.CfnOutput(this, 'bamLambda', {
            value: bamLambda.functionName,
        });
    }
}
