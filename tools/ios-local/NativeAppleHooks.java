import java.util.*;

/** Bootstrap-visible hook methods, without dependencies on Kotlin or ASM. */
public final class NativeAppleHooks {
    private static Object ios(ClassLoader loader) throws ReflectiveOperationException {
        return Class.forName("org.jetbrains.kotlin.konan.target.KonanTarget$IOS_ARM64", true, loader)
            .getField("INSTANCE").get(null);
    }
    public static List<?> enableApple(List<?> enabled) {
        try {
            Object ios = ios(enabled.get(0).getClass().getClassLoader());
            ArrayList<Object> result = new ArrayList<>(enabled);
            if (!result.contains(ios)) result.add(ios);
            return result;
        } catch (ReflectiveOperationException failure) { throw new IllegalStateException(failure); }
    }
    public static boolean enableFlag(boolean original, Object target) {
        return original || target.toString().equals("ios_arm64");
    }
    public static Map<?, ?> enableHost(Map<?, ?> hosts) {
        try {
            Object first = hosts.keySet().iterator().next();
            Object ios = ios(first.getClass().getClassLoader());
            Map<Object, Object> result = new LinkedHashMap<>(hosts);
            for (Map.Entry<?, ?> entry : hosts.entrySet()) {
                if (entry.getKey().toString().equals("linux_x64")) {
                    Set<Object> targets = new LinkedHashSet<>((Set<?>) entry.getValue());
                    targets.add(ios);
                    result.put(entry.getKey(), targets);
                }
            }
            return result;
        } catch (ReflectiveOperationException failure) { throw new IllegalStateException(failure); }
    }
}
