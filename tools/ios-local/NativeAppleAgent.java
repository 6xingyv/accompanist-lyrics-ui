import java.lang.instrument.*;
import java.security.ProtectionDomain;
import java.util.*;
import org.jetbrains.org.objectweb.asm.*;
import org.jetbrains.org.objectweb.asm.tree.*;

/** Experimental, process-local adaptation. Does not modify the installed Kotlin distribution. */
public final class NativeAppleAgent {
    private static boolean installed;
    public static void premain(String arguments, Instrumentation instrumentation) {
        if (installed) return;
        installed = true;
        if (System.getenv("ACCOMPANIST_IOS_SYSROOT") == null) throw new IllegalStateException("Missing iOS SDK");
        // Gradle's isolated plugin loaders filter application classes. Make the small
        // hook methods visible to those loaders without modifying plugin JARs.
        try {
            java.io.File agent = new java.io.File(NativeAppleAgent.class.getProtectionDomain().getCodeSource().getLocation().toURI());
            instrumentation.appendToBootstrapClassLoaderSearch(new java.util.jar.JarFile(
                new java.io.File(agent.getParentFile(), "native-apple-hooks.jar")));
        } catch (Exception failure) { throw new IllegalStateException(failure); }
        instrumentation.addTransformer(new ClassFileTransformer() {
            public byte[] transform(ClassLoader loader, String name, Class<?> type, ProtectionDomain domain, byte[] bytes) {
                if (!name.equals("org/jetbrains/kotlin/konan/target/HostManager") &&
                    !name.equals("org/jetbrains/kotlin/konan/target/AppleConfigurablesImpl")) return null;
                ClassNode node = new ClassNode();
                new ClassReader(bytes).accept(node, 0);
                for (MethodNode method : node.methods) {
                    if (name.endsWith("HostManager") && method.name.equals("isEnabled")) {
                        for (AbstractInsnNode instruction : method.instructions.toArray()) {
                            if (instruction.getOpcode() == Opcodes.IRETURN) {
                                InsnList hook = new InsnList();
                                hook.add(new VarInsnNode(Opcodes.ALOAD, 1));
                                hook.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "NativeAppleHooks", "enableFlag",
                                    "(ZLjava/lang/Object;)Z", false));
                                method.instructions.insertBefore(instruction, hook);
                            }
                        }
                    }
                    if (name.endsWith("HostManager") && method.name.equals("getEnabled")) {
                        for (AbstractInsnNode instruction : method.instructions.toArray()) {
                            if (instruction.getOpcode() == Opcodes.ARETURN) method.instructions.insertBefore(instruction,
                                new MethodInsnNode(Opcodes.INVOKESTATIC, "NativeAppleHooks", "enableApple",
                                    "(Ljava/util/List;)Ljava/util/List;", false));
                        }
                    }
                    if (name.endsWith("HostManager") && method.name.equals("getEnabledByHost")) {
                        for (AbstractInsnNode instruction : method.instructions.toArray()) {
                            if (instruction.getOpcode() == Opcodes.ARETURN) method.instructions.insertBefore(instruction,
                                new MethodInsnNode(Opcodes.INVOKESTATIC, "NativeAppleHooks", "enableHost",
                                    "(Ljava/util/Map;)Ljava/util/Map;", false));
                        }
                    }
                    String variable = switch (method.name) {
                        case "getAbsoluteTargetSysRoot" -> "ACCOMPANIST_IOS_SYSROOT";
                        case "getAbsoluteTargetToolchain", "getAbsoluteAdditionalToolsDir" -> "ACCOMPANIST_IOS_TOOLCHAIN";
                        default -> null;
                    };
                    if (variable != null && name.endsWith("AppleConfigurablesImpl")) {
                        method.instructions.clear(); method.tryCatchBlocks.clear();
                        if (method.localVariables != null) method.localVariables.clear();
                        method.instructions.add(new LdcInsnNode(variable));
                        method.instructions.add(new MethodInsnNode(Opcodes.INVOKESTATIC, "java/lang/System", "getenv",
                            "(Ljava/lang/String;)Ljava/lang/String;", false));
                        method.instructions.add(new InsnNode(Opcodes.ARETURN));
                    }
                }
                ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_MAXS);
                node.accept(writer);
                System.err.println("[local-ios] Adapted " + name);
                return writer.toByteArray();
            }
        });
    }
}
