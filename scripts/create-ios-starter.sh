#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
info()  { echo -e "${CYAN}[INFO]${NC}  $*"; }
ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
err()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

app_name=""
organization_id="com.example"
bundle_id=""
team_id=""
interface_style="swiftui"
deployment_target="17.0"
output_root="$PROJECT_ROOT/generated-ios"
create_archive=false

usage() {
  cat <<'EOF'
Usage: ./scripts/create-ios-starter.sh --name <AppName> [options]

Options:
  --name <name>                 Display name for the generated app (required)
  --organization-id <id>       Reverse-DNS org id used for default bundle id
  --bundle-id <id>             Explicit bundle identifier
  --team-id <id>               Apple Developer team id for automatic signing
  --interface <swiftui|uikit>  Starter interface style (default: swiftui)
  --deployment-target <ver>    iOS deployment target (default: 17.0)
  --output-dir <path>          Root directory for generated projects
  --archive                    Create a zip archive next to the project folder
  -h, --help                   Show this help

Examples:
  ./scripts/create-ios-starter.sh --name EdgeStarter
  ./scripts/create-ios-starter.sh --name FieldOps --interface uikit --team-id ABC123XYZ9 --archive
EOF
}

trim() {
  local value="$1"
  printf '%s' "$value" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//'
}

lowercase() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

sanitize_module_name() {
  local raw="$1"
  local sanitized
  sanitized="$(printf '%s' "$raw" | tr -cd '[:alnum:]_')"
  if [[ -z "$sanitized" ]]; then
    err "App name must contain at least one alphanumeric character."
    exit 1
  fi
  printf '%s' "$sanitized"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name)
      app_name="${2:-}"
      shift 2
      ;;
    --organization-id)
      organization_id="${2:-}"
      shift 2
      ;;
    --bundle-id)
      bundle_id="${2:-}"
      shift 2
      ;;
    --team-id)
      team_id="${2:-}"
      shift 2
      ;;
    --interface)
      interface_style="${2:-}"
      shift 2
      ;;
    --deployment-target)
      deployment_target="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_root="${2:-}"
      shift 2
      ;;
    --archive)
      create_archive=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      err "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

app_name="$(trim "$app_name")"
organization_id="$(trim "$organization_id")"
bundle_id="$(trim "$bundle_id")"
team_id="$(trim "$team_id")"
output_root="$(trim "$output_root")"

if [[ -z "$app_name" ]]; then
  err "--name is required"
  usage
  exit 1
fi

case "$interface_style" in
  swiftui|uikit)
    ;;
  *)
    err "--interface must be one of: swiftui, uikit"
    exit 1
    ;;
esac

module_name="$(sanitize_module_name "$app_name")"
module_name_lower="$(lowercase "$module_name")"

if [[ -z "$bundle_id" ]]; then
  bundle_id="${organization_id}.${module_name_lower}"
fi

project_dir="$output_root/$module_name"
source_dir="$project_dir/$module_name"
project_file_dir="$project_dir/$module_name.xcodeproj"
scheme_dir="$project_file_dir/xcshareddata/xcschemes"
project_file="$project_file_dir/project.pbxproj"
scheme_file="$scheme_dir/$module_name.xcscheme"
readme_file="$project_dir/README.md"

if [[ -e "$project_dir" ]]; then
  err "Destination already exists: $project_dir"
  exit 1
fi

mkdir -p "$source_dir" "$scheme_dir"

if [[ -n "$team_id" ]]; then
  code_sign_allowed="YES"
  code_sign_required="YES"
  code_sign_identity='CODE_SIGN_IDENTITY = "Apple Development";'
else
  code_sign_allowed="NO"
  code_sign_required="NO"
  code_sign_identity=""
fi

write_swiftui_files() {
  cat > "$source_dir/App.swift" <<EOF
import SwiftUI

@main
struct ${module_name}App: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
EOF

  cat > "$source_dir/ContentView.swift" <<EOF
import SwiftUI

struct ContentView: View {
    private let cards = ProjectInfo.starterCards

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    hero

                    ForEach(cards) { card in
                        VStack(alignment: .leading, spacing: 8) {
                            Label(card.title, systemImage: card.icon)
                                .font(.headline)
                            Text(card.detail)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(16)
                        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    }
                }
                .padding(20)
            }
            .background(
                LinearGradient(
                    colors: [Color.orange.opacity(0.22), Color.blue.opacity(0.16), Color.cyan.opacity(0.14)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .ignoresSafeArea()
            )
            .navigationTitle(ProjectInfo.displayName)
        }
    }

    private var hero: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Hello from \(ProjectInfo.displayName)")
                .font(.system(size: 30, weight: .bold, design: .rounded))
            Text("Generated by EdgeFabric with the \(ProjectInfo.interfaceLabel) starter profile.")
                .font(.body)
                .foregroundStyle(.secondary)

            HStack(spacing: 12) {
                stat(title: "Interface", value: ProjectInfo.interfaceLabel)
                stat(title: "Target", value: "iPhone + iPad")
                stat(title: "Bundle", value: ProjectInfo.bundleId)
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.white.opacity(0.76), in: RoundedRectangle(cornerRadius: 24, style: .continuous))
    }

    private func stat(title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title.uppercased())
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.weight(.semibold))
                .lineLimit(1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(Color.black.opacity(0.06), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

#Preview {
    ContentView()
}
EOF
}

write_uikit_files() {
  cat > "$source_dir/App.swift" <<EOF
import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(name: "Default Configuration", sessionRole: connectingSceneSession.role)
        configuration.delegateClass = SceneDelegate.self
        return configuration
    }
}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = UINavigationController(rootViewController: ViewController())
        self.window = window
        window.makeKeyAndVisible()
    }
}
EOF

  cat > "$source_dir/ContentView.swift" <<EOF
import UIKit

final class ViewController: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        title = ProjectInfo.displayName
        view.backgroundColor = UIColor.systemBackground
        configureLayout()
    }

    private func configureLayout() {
        let titleLabel = UILabel()
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        titleLabel.font = .systemFont(ofSize: 30, weight: .bold)
        titleLabel.text = ProjectInfo.displayName
        titleLabel.numberOfLines = 0

        let subtitleLabel = UILabel()
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.font = .systemFont(ofSize: 17, weight: .regular)
        subtitleLabel.textColor = .secondaryLabel
        subtitleLabel.numberOfLines = 0
        subtitleLabel.text = "Generated by EdgeFabric with the \(ProjectInfo.interfaceLabel) starter profile."

        let stack = UIStackView(arrangedSubviews: [titleLabel, subtitleLabel])
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.axis = .vertical
        stack.spacing = 14

        let cardStack = UIStackView(arrangedSubviews: ProjectInfo.starterCards.map(makeCard))
        cardStack.translatesAutoresizingMaskIntoConstraints = false
        cardStack.axis = .vertical
        cardStack.spacing = 12

        let container = UIStackView(arrangedSubviews: [stack, cardStack])
        container.translatesAutoresizingMaskIntoConstraints = false
        container.axis = .vertical
        container.spacing = 24

        view.addSubview(container)
        NSLayoutConstraint.activate([
            container.leadingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.leadingAnchor, constant: 20),
            container.trailingAnchor.constraint(equalTo: view.safeAreaLayoutGuide.trailingAnchor, constant: -20),
            container.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 24)
        ])
    }

    private func makeCard(for card: StarterCard) -> UIView {
        let titleLabel = UILabel()
        titleLabel.font = .preferredFont(forTextStyle: .headline)
        titleLabel.text = card.title

        let detailLabel = UILabel()
        detailLabel.font = .preferredFont(forTextStyle: .subheadline)
        detailLabel.textColor = .secondaryLabel
        detailLabel.numberOfLines = 0
        detailLabel.text = card.detail

        let stack = UIStackView(arrangedSubviews: [titleLabel, detailLabel])
        stack.axis = .vertical
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false

        let wrapper = UIView()
        wrapper.backgroundColor = .secondarySystemBackground
        wrapper.layer.cornerRadius = 18
        wrapper.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: wrapper.leadingAnchor, constant: 16),
            stack.trailingAnchor.constraint(equalTo: wrapper.trailingAnchor, constant: -16),
            stack.topAnchor.constraint(equalTo: wrapper.topAnchor, constant: 14),
            stack.bottomAnchor.constraint(equalTo: wrapper.bottomAnchor, constant: -14)
        ])

        return wrapper
    }
}
EOF
}

cat > "$source_dir/ProjectInfo.swift" <<EOF
import Foundation

struct StarterCard: Identifiable {
    let id = UUID()
    let title: String
    let detail: String
    let icon: String
}

enum ProjectInfo {
    static let displayName = "${app_name}"
    static let bundleId = "${bundle_id}"
    static let interfaceLabel = "${interface_style}"
    static let starterCards: [StarterCard] = [
        StarterCard(
            title: "Starter Ready",
            detail: "This project was generated inside EdgeFabric and is ready to open in Xcode or VS Code.",
            icon: "sparkles"
        ),
        StarterCard(
            title: "Signing",
            detail: "${team_id:-No Apple team assigned yet. Update the team id when you are ready for device signing.}",
            icon: "person.crop.rectangle"
        ),
        StarterCard(
            title: "Deployment Target",
            detail: "iOS ${deployment_target}",
            icon: "iphone.gen3"
        ),
    ]
}
EOF

if [[ "$interface_style" == "swiftui" ]]; then
  write_swiftui_files
else
  write_uikit_files
fi

cat > "$project_file" <<EOF
// !\$*UTF8*\$!
{
	archiveVersion = 1;
	classes = {
	};
	objectVersion = 56;
	objects = {

/* Begin PBXBuildFile section */
		A00000010000000000000001 /* App.swift in Sources */ = {isa = PBXBuildFile; fileRef = A00000020000000000000001 /* App.swift */; };
		A00000010000000000000002 /* ContentView.swift in Sources */ = {isa = PBXBuildFile; fileRef = A00000020000000000000002 /* ContentView.swift */; };
		A00000010000000000000003 /* ProjectInfo.swift in Sources */ = {isa = PBXBuildFile; fileRef = A00000020000000000000003 /* ProjectInfo.swift */; };
/* End PBXBuildFile section */

/* Begin PBXFileReference section */
		A00000020000000000000001 /* App.swift */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = App.swift; sourceTree = "<group>"; };
		A00000020000000000000002 /* ContentView.swift */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = ContentView.swift; sourceTree = "<group>"; };
		A00000020000000000000003 /* ProjectInfo.swift */ = {isa = PBXFileReference; lastKnownFileType = sourcecode.swift; path = ProjectInfo.swift; sourceTree = "<group>"; };
		A00000020000000000000004 /* ${module_name}.app */ = {isa = PBXFileReference; explicitFileType = wrapper.application; includeInIndex = 0; path = ${module_name}.app; sourceTree = BUILT_PRODUCTS_DIR; };
/* End PBXFileReference section */

/* Begin PBXFrameworksBuildPhase section */
		A00000030000000000000001 /* Frameworks */ = {
			isa = PBXFrameworksBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXFrameworksBuildPhase section */

/* Begin PBXGroup section */
		A00000040000000000000001 = {
			isa = PBXGroup;
			children = (
				A00000040000000000000002 /* ${module_name} */,
				A00000040000000000000003 /* Products */,
			);
			sourceTree = "<group>";
		};
		A00000040000000000000002 /* ${module_name} */ = {
			isa = PBXGroup;
			children = (
				A00000020000000000000001 /* App.swift */,
				A00000020000000000000002 /* ContentView.swift */,
				A00000020000000000000003 /* ProjectInfo.swift */,
			);
			path = ${module_name};
			sourceTree = "<group>";
		};
		A00000040000000000000003 /* Products */ = {
			isa = PBXGroup;
			children = (
				A00000020000000000000004 /* ${module_name}.app */,
			);
			name = Products;
			sourceTree = "<group>";
		};
/* End PBXGroup section */

/* Begin PBXNativeTarget section */
		A00000050000000000000001 /* ${module_name} */ = {
			isa = PBXNativeTarget;
			buildConfigurationList = A00000060000000000000002 /* Build configuration list for PBXNativeTarget "${module_name}" */;
			buildPhases = (
				A00000030000000000000002 /* Sources */,
				A00000030000000000000001 /* Frameworks */,
				A00000030000000000000003 /* Resources */,
			);
			buildRules = (
			);
			dependencies = (
			);
			name = ${module_name};
			productName = ${module_name};
			productReference = A00000020000000000000004 /* ${module_name}.app */;
			productType = "com.apple.product-type.application";
		};
/* End PBXNativeTarget section */

/* Begin PBXProject section */
		A00000070000000000000001 /* Project object */ = {
			isa = PBXProject;
			attributes = {
				BuildIndependentTargetsInParallel = 1;
				LastSwiftUpdateCheck = 1650;
				LastUpgradeCheck = 1650;
				TargetAttributes = {
					A00000050000000000000001 = {
						CreatedOnToolsVersion = 16.4;
					};
				};
			};
			buildConfigurationList = A00000060000000000000001 /* Build configuration list for PBXProject "${module_name}" */;
			compatibilityVersion = "Xcode 14.0";
			developmentRegion = en;
			hasScannedForEncodings = 0;
			knownRegions = (
				en,
				Base,
			);
			mainGroup = A00000040000000000000001;
			productRefGroup = A00000040000000000000003 /* Products */;
			projectDirPath = "";
			projectRoot = "";
			targets = (
				A00000050000000000000001 /* ${module_name} */,
			);
		};
/* End PBXProject section */

/* Begin PBXResourcesBuildPhase section */
		A00000030000000000000003 /* Resources */ = {
			isa = PBXResourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXResourcesBuildPhase section */

/* Begin PBXSourcesBuildPhase section */
		A00000030000000000000002 /* Sources */ = {
			isa = PBXSourcesBuildPhase;
			buildActionMask = 2147483647;
			files = (
				A00000010000000000000001 /* App.swift in Sources */,
				A00000010000000000000002 /* ContentView.swift in Sources */,
				A00000010000000000000003 /* ProjectInfo.swift in Sources */,
			);
			runOnlyForDeploymentPostprocessing = 0;
		};
/* End PBXSourcesBuildPhase section */

/* Begin XCBuildConfiguration section */
		A00000080000000000000001 /* Debug */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = dwarf;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_C_LANGUAGE_STANDARD = gnu17;
				GENERATE_INFOPLIST_FILE = YES;
				IPHONEOS_DEPLOYMENT_TARGET = ${deployment_target};
				SDKROOT = iphoneos;
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			};
			name = Debug;
		};
		A00000080000000000000002 /* Release */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				ALWAYS_SEARCH_USER_PATHS = NO;
				CLANG_ENABLE_MODULES = YES;
				CLANG_ENABLE_OBJC_ARC = YES;
				COPY_PHASE_STRIP = NO;
				DEBUG_INFORMATION_FORMAT = "dwarf-with-dsym";
				ENABLE_NS_ASSERTIONS = NO;
				ENABLE_STRICT_OBJC_MSGSEND = YES;
				GCC_C_LANGUAGE_STANDARD = gnu17;
				GENERATE_INFOPLIST_FILE = YES;
				IPHONEOS_DEPLOYMENT_TARGET = ${deployment_target};
				SDKROOT = iphoneos;
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			};
			name = Release;
		};
		A00000080000000000000003 /* Debug */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS = YES;
				CODE_SIGNING_ALLOWED = ${code_sign_allowed};
				CODE_SIGNING_REQUIRED = ${code_sign_required};
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				DEVELOPMENT_TEAM = ${team_id};
				${code_sign_identity}
				ENABLE_PREVIEWS = YES;
				GENERATE_INFOPLIST_FILE = YES;
				INFOPLIST_KEY_CFBundleDisplayName = ${app_name};
				INFOPLIST_KEY_UIApplicationSceneManifest_Generation = YES;
				INFOPLIST_KEY_UIApplicationSupportsIndirectInputEvents = YES;
				INFOPLIST_KEY_UILaunchScreen_Generation = YES;
				LD_RUNPATH_SEARCH_PATHS = (
					"$(inherited)",
					"@executable_path/Frameworks",
				);
				MARKETING_VERSION = 1.0;
				PRODUCT_BUNDLE_IDENTIFIER = ${bundle_id};
				PRODUCT_NAME = "$(TARGET_NAME)";
				SUPPORTED_PLATFORMS = "iphoneos iphonesimulator";
				SWIFT_EMIT_LOC_STRINGS = YES;
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			};
			name = Debug;
		};
		A00000080000000000000004 /* Release */ = {
			isa = XCBuildConfiguration;
			buildSettings = {
				ASSETCATALOG_COMPILER_GENERATE_SWIFT_ASSET_SYMBOL_EXTENSIONS = YES;
				CODE_SIGNING_ALLOWED = ${code_sign_allowed};
				CODE_SIGNING_REQUIRED = ${code_sign_required};
				CODE_SIGN_STYLE = Automatic;
				CURRENT_PROJECT_VERSION = 1;
				DEVELOPMENT_TEAM = ${team_id};
				${code_sign_identity}
				GENERATE_INFOPLIST_FILE = YES;
				INFOPLIST_KEY_CFBundleDisplayName = ${app_name};
				INFOPLIST_KEY_UIApplicationSceneManifest_Generation = YES;
				INFOPLIST_KEY_UIApplicationSupportsIndirectInputEvents = YES;
				INFOPLIST_KEY_UILaunchScreen_Generation = YES;
				LD_RUNPATH_SEARCH_PATHS = (
					"$(inherited)",
					"@executable_path/Frameworks",
				);
				MARKETING_VERSION = 1.0;
				PRODUCT_BUNDLE_IDENTIFIER = ${bundle_id};
				PRODUCT_NAME = "$(TARGET_NAME)";
				SUPPORTED_PLATFORMS = "iphoneos iphonesimulator";
				SWIFT_EMIT_LOC_STRINGS = YES;
				SWIFT_VERSION = 5.0;
				TARGETED_DEVICE_FAMILY = "1,2";
			};
			name = Release;
		};
/* End XCBuildConfiguration section */

/* Begin XCConfigurationList section */
		A00000060000000000000001 /* Build configuration list for PBXProject "${module_name}" */ = {
			isa = XCConfigurationList;
			buildConfigurations = (
				A00000080000000000000001 /* Debug */,
				A00000080000000000000002 /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		};
		A00000060000000000000002 /* Build configuration list for PBXNativeTarget "${module_name}" */ = {
			isa = XCConfigurationList;
			buildConfigurations = (
				A00000080000000000000003 /* Debug */,
				A00000080000000000000004 /* Release */,
			);
			defaultConfigurationIsVisible = 0;
			defaultConfigurationName = Release;
		};
/* End XCConfigurationList section */
	};
	rootObject = A00000070000000000000001 /* Project object */;
}
EOF

cat > "$scheme_file" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<Scheme
   LastUpgradeVersion = "1650"
   version = "1.7">
   <BuildAction
      parallelizeBuildables = "YES"
      buildImplicitDependencies = "YES">
      <BuildActionEntries>
         <BuildActionEntry
            buildForTesting = "YES"
            buildForRunning = "YES"
            buildForProfiling = "YES"
            buildForArchiving = "YES"
            buildForAnalyzing = "YES">
            <BuildableReference
               BuildableIdentifier = "primary"
               BlueprintIdentifier = "A00000050000000000000001"
               BuildableName = "${module_name}.app"
               BlueprintName = "${module_name}"
               ReferencedContainer = "container:${module_name}.xcodeproj">
            </BuildableReference>
         </BuildActionEntry>
      </BuildActionEntries>
   </BuildAction>
   <TestAction
      buildConfiguration = "Debug"
      selectedDebuggerIdentifier = "Xcode.DebuggerFoundation.Debugger.LLDB"
      selectedLauncherIdentifier = "Xcode.DebuggerFoundation.Launcher.LLDB"
      shouldUseLaunchSchemeArgsEnv = "YES">
      <Testables>
      </Testables>
   </TestAction>
   <LaunchAction
      buildConfiguration = "Debug"
      selectedDebuggerIdentifier = "Xcode.DebuggerFoundation.Debugger.LLDB"
      selectedLauncherIdentifier = "Xcode.DebuggerFoundation.Launcher.LLDB"
      launchStyle = "0"
      useCustomWorkingDirectory = "NO"
      ignoresPersistentStateOnLaunch = "NO"
      debugDocumentVersioning = "YES"
      debugServiceExtension = "internal"
      allowLocationSimulation = "YES">
      <BuildableProductRunnable
         runnableDebuggingMode = "0">
         <BuildableReference
            BuildableIdentifier = "primary"
            BlueprintIdentifier = "A00000050000000000000001"
            BuildableName = "${module_name}.app"
            BlueprintName = "${module_name}"
            ReferencedContainer = "container:${module_name}.xcodeproj">
         </BuildableReference>
      </BuildableProductRunnable>
   </LaunchAction>
   <ProfileAction
      buildConfiguration = "Release"
      shouldUseLaunchSchemeArgsEnv = "YES"
      savedToolIdentifier = ""
      useCustomWorkingDirectory = "NO"
      debugDocumentVersioning = "YES">
      <BuildableProductRunnable
         runnableDebuggingMode = "0">
         <BuildableReference
            BuildableIdentifier = "primary"
            BlueprintIdentifier = "A00000050000000000000001"
            BuildableName = "${module_name}.app"
            BlueprintName = "${module_name}"
            ReferencedContainer = "container:${module_name}.xcodeproj">
         </BuildableReference>
      </BuildableProductRunnable>
   </ProfileAction>
   <AnalyzeAction
      buildConfiguration = "Debug">
   </AnalyzeAction>
   <ArchiveAction
      buildConfiguration = "Release"
      revealArchiveInOrganizer = "YES">
   </ArchiveAction>
</Scheme>
EOF

cat > "$readme_file" <<EOF
# ${module_name}

Generated by EdgeFabric starter scaffolding.

## Selected Options

- Display name: ${app_name}
- Interface: ${interface_style}
- Bundle id: ${bundle_id}
- Deployment target: iOS ${deployment_target}
- Team id: ${team_id:-not assigned}

## Open In Xcode

Open ${module_name}.xcodeproj and run the shared ${module_name} scheme.

## Build For A Booted Simulator

\
\
```bash
xcodebuild -project ${module_name}.xcodeproj -scheme ${module_name} -destination 'id=<simulator-udid>' build
```
EOF

if [[ "$create_archive" == true ]]; then
  archive_path="$output_root/${module_name}.zip"
  rm -f "$archive_path"
  if command -v ditto &>/dev/null; then
    ditto -c -k --sequesterRsrc --keepParent "$project_dir" "$archive_path"
  else
    (cd "$output_root" && zip -qr "$archive_path" "$module_name")
  fi
  ok "Archive created at $archive_path"
fi

ok "Starter project created at $project_dir"
info "Open: $project_file_dir"
