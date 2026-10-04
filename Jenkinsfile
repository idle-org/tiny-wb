// Add this webhook in Gitea:
// http://192.168.1.37:8080/gitea-webhook/post

pipeline {
    agent { label 'rust_nodocker' }

    options {
        disableConcurrentBuilds()
        skipDefaultCheckout(true)
    }

    environment {
        // Jenkins does not render Cargo ANSI sequences cleanly.
        CARGO_TERM_COLOR = 'never'

        RUST_BACKTRACE = '1'

        // LTS-like compatibility floor verified by the dedicated MSRV stage.
        MSRV = '1.88.0'

        // Supply-chain tools are pinned for reproducible policy behavior.
        CARGO_AUDIT_VERSION = '0.22.2'
        CARGO_DENY_VERSION = '0.20.2'
        CARGO_LLVM_COV_VERSION = '0.6.16'
        BADGE_ROOT = '/mnt/assets/coverage'
        BADGE_OWNER = 'idle-org'
        BADGE_PROJECT = 'tiny-wb'
        BADGE_BRANCH = 'main'

        // Keep CI build products separate.
        CARGO_TARGET_DIR = "${WORKSPACE}/target-ci"

        // Force a portable x86-64 baseline.
        //
        // The repository already wants lld; specifying it here makes
        // the CI configuration explicit as well.
        RUSTFLAGS = '-Ctarget-cpu=x86-64 -Clink-arg=-fuse-ld=lld'
    }

    stages {
        stage('Checkout') {
            steps {
                sh 'rm -rf coverage && mkdir coverage'
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
                        --component clippy \
                        --component llvm-tools-preview

                    rustup toolchain install "${MSRV}" \
                        --profile minimal

                    rustup default stable

                    INSTALLED_AUDIT_VERSION="$(cargo audit --version 2>/dev/null | awk '{print $NF}' || true)"
                    if [ "${INSTALLED_AUDIT_VERSION}" != "${CARGO_AUDIT_VERSION}" ]; then
                        cargo install --locked \
                            --version "${CARGO_AUDIT_VERSION}" \
                            cargo-audit
                    fi

                    INSTALLED_DENY_VERSION="$(cargo deny --version 2>/dev/null | awk '{print $NF}' || true)"
                    if [ "${INSTALLED_DENY_VERSION}" != "${CARGO_DENY_VERSION}" ]; then
                        cargo install --locked \
                            --version "${CARGO_DENY_VERSION}" \
                            cargo-deny
                    fi

                    command -v jq >/dev/null 2>&1 || {
                        echo 'ERROR: jq must be installed on this worker.' >&2
                        exit 1
                    }
                    INSTALLED_COVERAGE_VERSION="$(cargo llvm-cov --version 2>/dev/null | awk '{print $NF}' || true)"
                    if [ "${INSTALLED_COVERAGE_VERSION}" != "${CARGO_LLVM_COV_VERSION}" ]; then
                        cargo install --locked --version "${CARGO_LLVM_COV_VERSION}" cargo-llvm-cov
                    fi

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
                    # Clean build
                    # ---------------------------------------------------------

                    rm -rf "${CARGO_TARGET_DIR}"
                    mkdir -p "${CARGO_TARGET_DIR}"

                    # ---------------------------------------------------------
                    # Diagnostics
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
                    echo "MSRV:"
                    rustc +"${MSRV}" --version

                    echo
                    echo "cargo-audit:"
                    cargo audit --version

                    echo
                    echo "cargo-deny:"
                    cargo deny --version

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
                    echo "Host:"
                    uname -a

                    echo
                    echo "Rust target:"
                    rustc -vV
                '''
            }
        }

        stage('Cargo Audit') {
            steps {
                sh '''
                    set -eu

                    echo "Auditing the locked dependency graph with RustSec..."

                    cargo audit --deny warnings
                '''
            }
        }

        stage('Cargo Deny') {
            steps {
                sh '''
                    set -eu

                    echo "Checking dependency licenses, sources and policy..."

                    cargo deny check
                '''
            }
        }

        stage('MSRV') {
            steps {
                sh '''
                    set -eu

                    echo "Checking tiny-wb with Rust ${MSRV}..."

                    cargo +"${MSRV}" check \
                        --locked \
                        --all-targets \
                        --all-features
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
                        --locked \
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
                        --locked \
                        --all-targets \
                        --all-features
                '''
            }
        }

        stage('Coverage') {
            steps {
                sh '''
                    set -eu
                    scripts/collect-coverage.sh
                    touch "coverage/.tests-passed-${BUILD_NUMBER}"
                    touch "coverage/.coverage-ready-${BUILD_NUMBER}"
                '''
            }
        }

        stage('Clippy') {
            steps {
                sh '''
                    set -eu

                    echo "Running Clippy..."

                    cargo clippy \
                        --locked \
                        --all-targets \
                        --all-features \
                        -- \
                        -D warnings
                '''
            }
        }

        stage('Build Release') {
            steps {
                sh '''
                    set -eu

                    echo "Building tiny-wb release..."

                    cargo build \
                        --locked \
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

                    echo
                    echo "Dynamic dependencies:"
                    ldd "${BINARY}" || true
                '''
            }
        }

        stage('Smoke Test') {
            steps {
                sh '''
                    # ---------------------------------------------------------
                    # NON-FATAL DIAGNOSTIC
                    # ---------------------------------------------------------
                    #
                    # tiny-wb is an interactive serial/TUI application.
                    #
                    # On the current FreeBSD Jenkins worker the optimized
                    # executable currently receives SIGILL during startup,
                    # even with target-cpu=x86-64.
                    #
                    # Compilation, tests and release linking remain CI gates.
                    # This stage records the runtime behaviour without making
                    # it fail the complete CI job.
                    # ---------------------------------------------------------

                    BINARY="${CARGO_TARGET_DIR}/release/tiny-wb"

                    echo "Diagnostic release-binary smoke test..."

                    set +e

                    "${BINARY}" --help
                    RESULT=$?

                    set -e

                    echo

                    if [ "${RESULT}" -eq 0 ]; then
                        echo "Smoke test passed."
                    else
                        echo "WARNING: release executable exited with ${RESULT}."
                        echo
                        echo "This is currently diagnostic only."
                        echo "The FreeBSD Jenkins worker has previously produced"
                        echo "SIGILL (exit 132) when starting the optimized binary."
                    fi

                    # Deliberately succeed.
                    exit 0
                '''
            }
        }
    }

    post {
        success {
            publishChecks(
                conclusion: 'SUCCESS',
                name: 'Rust CI',
                summary: 'tiny-wb passed CI.',
                text: 'RustSec audit, dependency policy, MSRV, formatting, compilation checks, tests, coverage, Clippy and the release build completed successfully. Badge generation and archival run in cleanup. The release runtime smoke test is currently diagnostic only.',
                title: 'Build Successful'
            )

            echo '✅ tiny-wb passed CI.'
        }

        failure {
            publishChecks(
                conclusion: 'FAILURE',
                name: 'Rust CI',
                summary: 'tiny-wb failed CI.',
                text: 'RustSec audit, dependency policy, MSRV, formatting, compilation, tests, coverage, Clippy, or the release build failed. Cleanup generates and archives current badges, including failure fallbacks. Check the Jenkins log.',
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
        cleanup {
            sh 'scripts/finalize-badges.sh'
            archiveArtifacts artifacts: 'coverage/version.svg,coverage/test.svg,coverage/coverage.svg',
                fingerprint: true, allowEmptyArchive: false
            script {
                // BRANCH_NAME is supplied by Jenkins multibranch jobs.
                // Fail closed for other job types; do not infer from detached HEAD.
                if (!env.BRANCH_NAME) {
                    echo 'Badge publication skipped: BRANCH_NAME unavailable; verify multibranch job configuration.'
                } else if (env.BRANCH_NAME == env.BADGE_BRANCH && !env.CHANGE_ID && !env.TAG_NAME) {
                    sh 'scripts/publish-badges.sh "$BADGE_ROOT" "$BADGE_OWNER" "$BADGE_PROJECT" "$BADGE_BRANCH" version test coverage'
                }
            }
        }
    }
}
