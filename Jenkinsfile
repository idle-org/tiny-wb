// Add this webhook in Gitea:
// http://192.168.1.37:8080/gitea-webhook/post

pipeline {
    agent { label 'rust_no_docker' }

    environment {
        // Jenkins console currently does not render Cargo ANSI colors cleanly.
        CARGO_TERM_COLOR = 'never'

        RUST_BACKTRACE = '1'

        // Isolate CI from any global ~/.cargo/config* on the worker.
        CARGO_HOME = "${WORKSPACE}/.ci-cargo-home"

        // Keep CI build artifacts separate from normal/local builds.
        CARGO_TARGET_DIR = "${WORKSPACE}/target-ci"

        // Build for a generic x86-64 baseline to avoid SIGILL on the worker.
        // Keep the project's desired lld linker.
        RUSTFLAGS = '-Ctarget-cpu=x86-64 -Clink-arg=-fuse-ld=lld'
    }

    stages {
        stage('Checkout') {
            steps {
                checkout scm

                sh '''
                    set -eu

                    git submodule sync --recursive
                    git submodule update --init --recursive
                '''
            }
        }

        stage('Setup Rust') {
            steps {
                sh '''
                    set -eu

                    if ! command -v rustup >/dev/null 2>&1; then
                        echo "ERROR: rustup is not installed on this worker." >&2
                        exit 1
                    fi

                    echo "Ensuring stable Rust is available..."

                    rustup toolchain install stable \
                        --profile minimal \
                        --component rustfmt \
                        --component clippy

                    rustup default stable

                    # ---------------------------------------------------------
                    # LLD
                    # ---------------------------------------------------------

                    if ! command -v ld.lld >/dev/null 2>&1; then
                        echo "lld not found — installing it..."
                        pkg install -y lld
                    else
                        echo "lld already installed."
                    fi

                    # ---------------------------------------------------------
                    # Fresh isolated CI directories
                    # ---------------------------------------------------------

                    rm -rf "${CARGO_HOME}"
                    mkdir -p "${CARGO_HOME}"

                    rm -rf "${CARGO_TARGET_DIR}"
                    mkdir -p "${CARGO_TARGET_DIR}"

                    # ---------------------------------------------------------
                    # Environment information
                    # ---------------------------------------------------------

                    echo
                    echo "Rust:"
                    rustc --version

                    echo
                    echo "Cargo:"
                    cargo --version

                    echo
                    echo "rustfmt:"
                    rustfmt --version

                    echo
                    echo "Clippy:"
                    cargo clippy --version

                    echo
                    echo "Compiler:"
                    cc --version || true

                    echo
                    echo "LLD:"
                    ld.lld --version

                    echo
                    echo "Rust flags:"
                    echo "${RUSTFLAGS}"

                    echo
                    echo "Cargo color mode:"
                    echo "${CARGO_TERM_COLOR}"

                    echo
                    echo "Host:"
                    uname -a

                    echo
                    echo "Rust target:"
                    rustc -vV
                '''
            }
        }

        stage('Format') {
            steps {
                sh '''
                    set -eu

                    echo "Checking Rust formatting..."

                    cargo fmt \
                        --all \
                        -- \
                        --check
                '''
            }
        }

        stage('Check') {
            steps {
                sh '''
                    set -eu

                    echo "Checking tiny-wb..."

                    cargo check \
                        --all-targets \
                        --all-features
                '''
            }
        }

        stage('Tests') {
            steps {
                sh '''
                    set -eu

                    echo "Running tests..."

                    cargo test \
                        --all-targets \
                        --all-features
                '''
            }
        }

        stage('Clippy') {
            steps {
                sh '''
                    set -eu

                    echo "Running Clippy..."

                    cargo clippy \
                        --all-targets \
                        --all-features
                '''
            }
        }

        stage('Build Release') {
            steps {
                sh '''
                    set -eu

                    echo "Building tiny-wb release..."

                    cargo build \
                        --release \
                        --all-features

                    BINARY="${CARGO_TARGET_DIR}/release/tiny-wb"

                    if [ ! -f "${BINARY}" ]; then
                        echo "ERROR: expected release binary not found:" >&2
                        echo "${BINARY}" >&2
                        exit 1
                    fi

                    echo
                    echo "Release binary:"
                    ls -lh "${BINARY}"

                    echo
                    echo "Binary information:"
                    file "${BINARY}" || true
                '''
            }
        }

        stage('Smoke Test') {
            steps {
                sh '''
                    set -eu

                    BINARY="${CARGO_TARGET_DIR}/release/tiny-wb"

                    echo "Smoke-testing CLI..."

                    "${BINARY}" --help >/dev/null

                    echo "CLI smoke test passed."
                '''
            }
        }
    }

    post {
        success {
            publishChecks(
                conclusion: 'SUCCESS',
                name: 'Rust CI',
                summary: 'tiny-wb passed all CI checks.',
                text: 'Formatting, Rust checks, tests, Clippy, release build, and CLI smoke test completed successfully.',
                title: 'Build Successful'
            )

            echo '✅ tiny-wb passed all CI checks.'
        }

        failure {
            publishChecks(
                conclusion: 'FAILURE',
                name: 'Rust CI',
                summary: 'tiny-wb failed CI.',
                text: 'Formatting, compilation, tests, Clippy, release build, or CLI smoke test failed. Check the Jenkins log.',
                title: 'Build Failed'
            )

            echo '❌ tiny-wb CI failed.'
        }

        unstable {
            publishChecks(
                conclusion: 'ACTION_REQUIRED',
                name: 'Rust CI',
                summary: 'tiny-wb CI is unstable.',
                text: 'The Jenkins build completed with an unstable status.',
                title: 'Build Unstable'
            )

            echo '⚠️ tiny-wb CI is unstable.'
        }

        always {
            sh '''
                echo "Cleaning CI configuration..."

                rm -rf .ci-cargo-home

                # Keep target-ci so the produced binary remains available
                # for inspection after the Jenkins build.
            '''
        }
    }
}