@Library('jenkins-joylib@v1.0.4') _

pipeline {

    agent {
        label joyCommonLabels(image_ver: '19.4.0')
    }

    options {
        buildDiscarder(logRotator(numToKeepStr: '10'))
        timestamps()
    }

    stages {
        stage('check') {
            steps{
                sh('make check')
            }
        }
        stage('test') {
            steps{
                sh('make test')
            }
        }
    }
}
