use deptree_graph::GraphData;

/// Render Cytoscape graph data into the bundled HTML template.
pub fn render_cytoscape_html(graph_data: &GraphData) -> Result<String, Box<dyn std::error::Error>> {
    const TEMPLATE: &str = include_str!("../templates/cytoscape.html");

    let graph_json = serde_json::to_string(graph_data)?;
    let html = TEMPLATE.replace("<!--GRAPH_DATA_PLACEHOLDER-->", &graph_json);

    Ok(html)
}
