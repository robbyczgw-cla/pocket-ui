package dev.pocket.spike;

import android.content.Context;
import android.graphics.Rect;
import android.os.Bundle;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import org.json.JSONArray;
import org.json.JSONObject;

/** Virtual controls mirror the Rust hit map. Ordinary touch continues to the native surface underneath. */
final class SemanticView extends View {
    private JSONArray controls = new JSONArray();
    private final java.util.concurrent.ConcurrentLinkedQueue<Integer> actions = new java.util.concurrent.ConcurrentLinkedQueue<>();
    SemanticView(Context context) { super(context); setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES); }
    void publish(String json) {
        try { controls = new JSONArray(json); sendAccessibilityEvent(android.view.accessibility.AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED); }
        catch (Exception ignored) { }
    }
    String actions() {
        StringBuilder result = new StringBuilder(); Integer id;
        while ((id = actions.poll()) != null) { if (result.length() > 0) result.append(','); result.append(id); }
        return result.toString();
    }
    @Override public AccessibilityNodeProvider getAccessibilityNodeProvider() {
        return new AccessibilityNodeProvider() {
            @Override public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
                AccessibilityNodeInfo node = AccessibilityNodeInfo.obtain();
                node.setPackageName(getContext().getPackageName()); node.setEnabled(true); node.setVisibleToUser(isShown());
                if (id == View.NO_ID) {
                    node.setSource(SemanticView.this); node.setClassName("android.view.ViewGroup");
                    node.setBoundsInScreen(new Rect(0,0,getWidth(),getHeight()));
                    for (int i=0; i<controls.length(); i++) node.addChild(SemanticView.this,i);
                } else {
                    JSONObject control=controls.optJSONObject(id); if (control==null) return null;
                    node.setSource(SemanticView.this,id); node.setParent(SemanticView.this); node.setClassName("android.widget.Button");
                    node.setText(control.optString("label")); node.setContentDescription(control.optString("label"));
                    node.setSelected(control.optBoolean("selected")); node.setClickable(true); node.setFocusable(true); node.addAction(AccessibilityNodeInfo.ACTION_CLICK);
                    Rect bounds=new Rect(control.optInt("x"),control.optInt("y"),control.optInt("x")+control.optInt("w"),control.optInt("y")+control.optInt("h"));
                    node.setBoundsInParent(bounds); int[] origin=new int[2]; getLocationOnScreen(origin); bounds.offset(origin[0],origin[1]); node.setBoundsInScreen(bounds);
                }
                return node;
            }
            @Override public boolean performAction(int id,int action,Bundle args) {
                if (id>=0 && id<controls.length() && action==AccessibilityNodeInfo.ACTION_CLICK) {actions.add(id);return true;}
                return false;
            }
        };
    }
}
